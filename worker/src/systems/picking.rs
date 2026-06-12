use crate::ecs::ViewerWorld;
use nightshade::prelude::*;
use protocol::{SelectedEntity, WorkerMessage};

/// Requests a GPU pick at a pixel in physical canvas coordinates. The result
/// lands a frame later, once the renderer has read the entity id texture
/// back, so `apply` polls for it.
pub fn request(viewer_world: &mut ViewerWorld, world: &mut World, x: f32, y: f32) {
    world.resources.gpu_picking = GpuPicking::default();
    world
        .resources
        .gpu_picking
        .request_pick(x.max(0.0) as u32, y.max(0.0) as u32);
    viewer_world.resources.picking.pending = true;
}

/// Polls the pending pick. Hitting an entity selects it, the background
/// clears the selection.
pub fn apply(viewer_world: &mut ViewerWorld, world: &mut World) {
    if !viewer_world.resources.picking.pending {
        return;
    }
    let Some(result) = world.resources.gpu_picking.take_result() else {
        return;
    };
    viewer_world.resources.picking.pending = false;

    let entity = if result.depth > 0.0 {
        result.entity_id.and_then(|id| find_entity_by_id(world, id))
    } else {
        None
    };
    select(viewer_world, world, entity);
}

/// Sets the selection, syncs it to the engine's outline pass, and reports it
/// to the page.
pub fn select(viewer_world: &mut ViewerWorld, world: &mut World, entity: Option<Entity>) {
    viewer_world.resources.selection.selected = entity;
    world
        .resources
        .editor_selection
        .bounding_volume_selected_entity = entity;
    world.resources.editor_selection.selected_entities = entity.into_iter().collect();

    let detail = entity.map(|entity| SelectedEntity {
        id: entity.id,
        name: world
            .core
            .get_name(entity)
            .map(|name| name.0.clone())
            .unwrap_or_default(),
    });
    crate::post(&WorkerMessage::Selected { detail });
}

fn find_entity_by_id(world: &World, id: u32) -> Option<Entity> {
    world
        .core
        .query_entities(LOCAL_TRANSFORM)
        .find(|entity| entity.id == id)
}
