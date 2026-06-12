use crate::ecs::ViewerWorld;
use crate::systems::picking;
use nightshade::ecs::prefab::resources::mesh_cache_clear;
use nightshade::prelude::*;
use protocol::{AnimationCommand, ClipInfo, WorkerMessage};

/// Imports the converted GLB, spawns it, starts the first clip looping, and
/// queues a camera fit once transforms have settled.
pub fn load_glb(viewer_world: &mut ViewerWorld, world: &mut World, name: &str, bytes: &[u8]) {
    clear_model(viewer_world, world);

    let mut result = match nightshade::ecs::prefab::import_gltf_from_bytes(bytes) {
        Ok(result) => result,
        Err(error) => {
            crate::post(&WorkerMessage::ModelLoadFailed {
                error: error.to_string(),
            });
            return;
        }
    };

    let mesh_count = result.meshes.len() as u32;
    let skin_count = result.skins.len() as u32;
    let animation_count = result.animations.len() as u32;

    nightshade::ecs::loading::queue_gltf_load(world, &mut result);

    let animations = result.animations.clone();
    let skins = result.skins.clone();
    let mut clips: Vec<ClipInfo> = Vec::new();
    let mut roots: Vec<Entity> = Vec::new();

    for prefab in &result.prefabs {
        let entity = nightshade::ecs::prefab::spawn_prefab_with_skins(
            world,
            prefab,
            &animations,
            &skins,
            Vec3::zeros(),
        );
        if let Some(player) = world.core.get_animation_player_mut(entity) {
            if clips.is_empty() {
                clips = player
                    .clips
                    .iter()
                    .map(|clip| ClipInfo {
                        name: clip.name.clone(),
                        duration: clip.duration,
                    })
                    .collect();
            }
            if !player.clips.is_empty() {
                player.looping = true;
                player.play(0);
            }
        }
        roots.push(entity);
    }

    viewer_world.resources.viewer.model_roots = roots;
    viewer_world.resources.viewer.pending_fit_frames = 2;

    crate::post(&WorkerMessage::ModelLoaded {
        name: name.to_string(),
        clips,
        mesh_count,
        skin_count,
        animation_count,
    });
}

/// Despawns the loaded model and drops its meshes from the cache.
pub fn clear_model(viewer_world: &mut ViewerWorld, world: &mut World) {
    let roots = std::mem::take(&mut viewer_world.resources.viewer.model_roots);
    if roots.is_empty() {
        return;
    }
    for root in roots {
        despawn_recursive_immediate(world, root);
    }
    mesh_cache_clear(&mut world.resources.assets.mesh_cache);
    world.resources.mesh_render_state.request_full_rebuild();
    picking::select(viewer_world, world, None);
}

/// Drives the animation player on the loaded model.
pub fn apply_animation_command(
    viewer_world: &mut ViewerWorld,
    world: &mut World,
    command: AnimationCommand,
) {
    let Some(&root) = viewer_world.resources.viewer.model_roots.first() else {
        return;
    };
    let Some(player) = world.core.get_animation_player_mut(root) else {
        return;
    };
    match command {
        AnimationCommand::Play { index } => {
            player.play_all = false;
            player.play(index as usize);
        }
        AnimationCommand::PlayAll => {
            player.play_all = true;
            if player.current_clip.is_none() && !player.clips.is_empty() {
                player.play(0);
            }
            player.playing = true;
        }
        AnimationCommand::Pause => player.pause(),
        AnimationCommand::Resume => player.resume(),
        AnimationCommand::Stop => player.stop(),
        AnimationCommand::Seek { time } => player.time = time.max(0.0),
        AnimationCommand::SetSpeed { speed } => player.speed = speed,
        AnimationCommand::SetLooping { looping } => player.looping = looping,
    }
}

/// Per-frame: finishes the deferred camera fit and mirrors the animation
/// player state to the page.
pub fn tick(viewer_world: &mut ViewerWorld, world: &mut World) {
    let frames = viewer_world.resources.viewer.pending_fit_frames;
    if frames > 0 {
        let next = frames - 1;
        viewer_world.resources.viewer.pending_fit_frames = next;
        if next == 0 {
            nightshade::ecs::transform::systems::update_global_transforms_system(world);
            let roots = viewer_world.resources.viewer.model_roots.clone();
            frame_entities(world, &roots);
        }
    }

    if let Some(&root) = viewer_world.resources.viewer.model_roots.first()
        && let Some(player) = world.core.get_animation_player(root)
    {
        let duration = player
            .current_clip
            .and_then(|index| player.clips.get(index))
            .map(|clip| clip.duration)
            .unwrap_or(0.0);
        crate::post(&WorkerMessage::AnimationState {
            current: player.current_clip.map(|index| index as u32),
            playing: player.playing,
            time: player.time,
            duration,
            speed: player.speed,
            looping: player.looping,
        });
    }
}

fn frame_entities(world: &mut World, roots: &[Entity]) {
    let Some(camera_entity) = world.resources.active_camera else {
        return;
    };

    let mut min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut found_any = false;

    for &root in roots {
        accumulate_bounds(world, root, &mut min, &mut max, &mut found_any);
        for descendant in nightshade::ecs::transform::queries::query_descendants(world, root) {
            accumulate_bounds(world, descendant, &mut min, &mut max, &mut found_any);
        }
    }

    let fov_rad = world
        .core
        .get_camera(camera_entity)
        .and_then(|camera| match camera.projection {
            nightshade::ecs::camera::components::Projection::Perspective(perspective) => {
                Some(perspective.y_fov_rad)
            }
            _ => None,
        })
        .unwrap_or(45.0_f32.to_radians());

    let (center, radius, half_diagonal) = if found_any {
        let center = (min + max) * 0.5;
        let half_diagonal = ((max - min) * 0.5).norm().max(0.001);
        (
            center,
            half_diagonal / (fov_rad * 0.5).sin() * 1.2,
            half_diagonal,
        )
    } else {
        (Vec3::new(0.0, 1.0, 0.0), 4.0, 1.0)
    };

    if let Some(camera) = world.core.get_camera_mut(camera_entity)
        && let nightshade::ecs::camera::components::Projection::Perspective(perspective) =
            &mut camera.projection
    {
        perspective.z_near = (half_diagonal * 0.001).clamp(0.001, 0.1);
    }

    if let Some(pan_orbit) = world.core.get_pan_orbit_camera_mut(camera_entity) {
        pan_orbit.target_focus = center;
        pan_orbit.target_radius = radius;
        pan_orbit.target_yaw = 0.0;
        pan_orbit.target_pitch = 0.3;
        pan_orbit.limits.zoom_lower = half_diagonal * 0.001;
        pan_orbit.pan_distance = None;
    }
}

fn accumulate_bounds(
    world: &World,
    entity: Entity,
    min: &mut Vec3,
    max: &mut Vec3,
    found_any: &mut bool,
) {
    let Some(bounding_volume) = world.core.get_bounding_volume(entity) else {
        return;
    };
    let Some(global_transform) = world.core.get_global_transform(entity) else {
        return;
    };
    let world_obb = bounding_volume.obb.transform(&global_transform.0);
    for corner in world_obb.get_corners() {
        min.x = min.x.min(corner.x);
        min.y = min.y.min(corner.y);
        min.z = min.z.min(corner.z);
        max.x = max.x.max(corner.x);
        max.y = max.y.max(corner.y);
        max.z = max.z.max(corner.z);
        *found_any = true;
    }
}
