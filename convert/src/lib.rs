use nightshade::ecs::animation::components::{
    AnimationClip, AnimationProperty, AnimationSamplerOutput,
};
use nightshade::ecs::material::components::Material;
use nightshade::ecs::mesh::components::Mesh;
use nightshade::ecs::prefab::{GltfSkin, Prefab, PrefabNode};
use protocol::{BundleAnimation, BundleModel, BundleSummary};
use std::collections::HashMap;
use std::io::{Cursor, Read};

const FBX_TO_GLTF_SCALE: f32 = 0.01;
const MODEL_SIZE_THRESHOLD_BYTES: usize = 1_000_000;

pub struct Bundle {
    pub name: String,
    pub models: Vec<LoadedModel>,
    pub animations: Vec<LoadedAnimation>,
}

pub struct LoadedModel {
    pub name: String,
    pub prefab: Prefab,
    pub skins: Vec<GltfSkin>,
    pub meshes: HashMap<String, Mesh>,
    pub textures: HashMap<String, (Vec<u8>, u32, u32)>,
    pub node_count: usize,
    pub size_bytes: usize,
}

pub struct LoadedAnimation {
    pub name: String,
    pub clips: Vec<AnimationClip>,
    pub size_bytes: usize,
}

pub fn import_bundle(name: &str, zip_bytes: &[u8]) -> Result<(Bundle, BundleSummary), String> {
    let mut log = Vec::new();
    log.push(format!(
        "Importing '{}' ({} KB)",
        name,
        zip_bytes.len() / 1024
    ));

    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))
        .map_err(|error| format!("Failed to read zip: {error}"))?;

    let mut fbx_entries: Vec<(String, Vec<u8>)> = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| format!("Failed to read zip entry: {error}"))?;
        if file.is_dir() {
            continue;
        }
        let path = file.mangled_name();
        if path
            .components()
            .any(|component| component.as_os_str().to_string_lossy() == "__MACOSX")
        {
            continue;
        }
        let Some(extension) = path.extension() else {
            continue;
        };
        if !extension.eq_ignore_ascii_case("fbx") {
            continue;
        }
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mut bytes = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut bytes)
            .map_err(|error| format!("Failed to extract '{stem}': {error}"))?;
        fbx_entries.push((stem, bytes));
    }

    if fbx_entries.is_empty() {
        return Err("No FBX files found in the archive".to_string());
    }

    fbx_entries.sort_by_key(|entry| std::cmp::Reverse(entry.1.len()));
    log.push(format!("Found {} FBX files", fbx_entries.len()));

    let mut models = Vec::new();
    let mut animations = Vec::new();

    for (entry_name, bytes) in &fbx_entries {
        let is_model = bytes.len() > MODEL_SIZE_THRESHOLD_BYTES;
        if is_model {
            log.push(format!(
                "Loading model: {} ({} KB)",
                entry_name,
                bytes.len() / 1024
            ));
            match nightshade::ecs::prefab::import_fbx_from_bytes(bytes) {
                Ok(result) => {
                    log.push(format!(
                        "  Meshes: {}, skins: {}, textures: {}, nodes: {}",
                        result.meshes.len(),
                        result.skins.len(),
                        result.textures.len(),
                        result.node_count
                    ));
                    if let Some(prefab) = result.prefabs.into_iter().next() {
                        models.push(LoadedModel {
                            name: entry_name.clone(),
                            prefab,
                            skins: result.skins,
                            meshes: result.meshes,
                            textures: result.textures,
                            node_count: result.node_count,
                            size_bytes: bytes.len(),
                        });
                    } else {
                        log.push(format!("  No scene roots in '{entry_name}', skipped"));
                    }
                }
                Err(error) => {
                    log.push(format!("  Failed to load model '{entry_name}': {error}"));
                }
            }
        } else {
            match nightshade::ecs::prefab::import_fbx_animations_from_bytes(bytes) {
                Ok(clips) => {
                    if clips.is_empty() {
                        log.push(format!("  No clips in '{entry_name}', skipped"));
                    } else {
                        log.push(format!(
                            "Loaded animation: {} ({} clips, {:.2}s)",
                            entry_name,
                            clips.len(),
                            clips.first().map(|clip| clip.duration).unwrap_or(0.0)
                        ));
                        animations.push(LoadedAnimation {
                            name: entry_name.clone(),
                            clips,
                            size_bytes: bytes.len(),
                        });
                    }
                }
                Err(error) => {
                    log.push(format!(
                        "  Failed to load animation '{entry_name}': {error}"
                    ));
                }
            }
        }
    }

    if models.is_empty() {
        return Err("No character model found in the archive (no FBX above 1 MB)".to_string());
    }

    animations.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });

    log.push(format!(
        "Loaded {} models and {} animations",
        models.len(),
        animations.len()
    ));

    let summary = BundleSummary {
        name: name.to_string(),
        models: models
            .iter()
            .map(|model| BundleModel {
                name: model.name.clone(),
                mesh_count: model.meshes.len() as u32,
                skin_count: model.skins.len() as u32,
                texture_count: model.textures.len() as u32,
                node_count: model.node_count as u32,
                size_kb: (model.size_bytes / 1024) as u64,
            })
            .collect(),
        animations: animations
            .iter()
            .map(|animation| BundleAnimation {
                name: animation.name.clone(),
                duration: animation
                    .clips
                    .first()
                    .map(|clip| clip.duration)
                    .unwrap_or(0.0),
                channel_count: animation
                    .clips
                    .iter()
                    .map(|clip| clip.channels.len() as u32)
                    .sum(),
                size_kb: (animation.size_bytes / 1024) as u64,
            })
            .collect(),
        log: log.clone(),
    };

    let bundle = Bundle {
        name: name.to_string(),
        models,
        animations,
    };

    Ok((bundle, summary))
}

pub fn build_glb(
    bundle: &Bundle,
    model_index: usize,
    animation_indices: &[usize],
    strip_root_motion: bool,
) -> Result<(Vec<u8>, Vec<String>), String> {
    let model = bundle
        .models
        .get(model_index)
        .ok_or_else(|| format!("Model index {model_index} out of range"))?;

    let mut selected_clips: Vec<AnimationClip> = Vec::new();
    for &animation_index in animation_indices {
        let animation = bundle
            .animations
            .get(animation_index)
            .ok_or_else(|| format!("Animation index {animation_index} out of range"))?;
        for mut clip in animation.clips.clone() {
            if strip_root_motion {
                clip.channels
                    .retain(|channel| channel.target_property != AnimationProperty::Translation);
            }
            clip.name = animation.name.clone();
            selected_clips.push(clip);
        }
    }

    let mut log = Vec::new();
    log.push(format!(
        "Building GLB for '{}' with {} animations",
        model.name,
        selected_clips.len()
    ));
    let glb = build_glb_bytes(model, &selected_clips, &mut log).map_err(|error| {
        log.push(format!("Build failed: {error}"));
        format!("{error}")
    })?;
    Ok((glb, log))
}

fn build_glb_bytes(
    model: &LoadedModel,
    animations: &[AnimationClip],
    log: &mut Vec<String>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use gltf_json as json;
    use json::validation::USize64;

    let mut buffer_data: Vec<u8> = Vec::new();

    let mut accessors: Vec<json::Accessor> = Vec::new();
    let mut buffer_views: Vec<json::buffer::View> = Vec::new();
    let mut meshes: Vec<json::Mesh> = Vec::new();
    let mut gltf_nodes: Vec<json::Node> = Vec::new();
    let mut skins: Vec<json::Skin> = Vec::new();
    let mut images: Vec<json::Image> = Vec::new();
    let mut gltf_textures: Vec<json::Texture> = Vec::new();
    let mut materials: Vec<json::Material> = Vec::new();
    let mut samplers: Vec<json::texture::Sampler> = Vec::new();
    let mut texture_name_to_index: HashMap<String, u32> = HashMap::new();

    let mut node_index_map: HashMap<usize, usize> = HashMap::new();

    struct PrefabNodeInfo {
        gltf_index: usize,
        child_gltf_indices: Vec<usize>,
        name: Option<String>,
        translation: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    }

    fn assign_indices(
        prefab_node: &PrefabNode,
        node_index_map: &mut HashMap<usize, usize>,
        next_index: &mut usize,
        node_infos: &mut Vec<PrefabNodeInfo>,
    ) -> usize {
        let current_index = *next_index;
        *next_index += 1;

        if let Some(prefab_index) = prefab_node.node_index {
            node_index_map.insert(prefab_index, current_index);
        }

        let mut child_gltf_indices = Vec::new();
        for child in &prefab_node.children {
            let child_index = assign_indices(child, node_index_map, next_index, node_infos);
            child_gltf_indices.push(child_index);
        }

        node_infos.push(PrefabNodeInfo {
            gltf_index: current_index,
            child_gltf_indices,
            name: prefab_node
                .components
                .name
                .as_ref()
                .map(|name| name.0.clone()),
            translation: [
                prefab_node.local_transform.translation.x * FBX_TO_GLTF_SCALE,
                prefab_node.local_transform.translation.y * FBX_TO_GLTF_SCALE,
                prefab_node.local_transform.translation.z * FBX_TO_GLTF_SCALE,
            ],
            rotation: [
                prefab_node.local_transform.rotation.i,
                prefab_node.local_transform.rotation.j,
                prefab_node.local_transform.rotation.k,
                prefab_node.local_transform.rotation.w,
            ],
            scale: [
                prefab_node.local_transform.scale.x,
                prefab_node.local_transform.scale.y,
                prefab_node.local_transform.scale.z,
            ],
        });

        current_index
    }

    let mut node_infos: Vec<PrefabNodeInfo> = Vec::new();
    let mut next_index = 0usize;

    for root_node in &model.prefab.root_nodes {
        assign_indices(
            root_node,
            &mut node_index_map,
            &mut next_index,
            &mut node_infos,
        );
    }

    node_infos.sort_by_key(|info| info.gltf_index);

    for info in &node_infos {
        gltf_nodes.push(json::Node {
            name: info.name.clone(),
            translation: Some(info.translation),
            rotation: Some(json::scene::UnitQuaternion(info.rotation)),
            scale: Some(info.scale),
            mesh: None,
            skin: None,
            children: if info.child_gltf_indices.is_empty() {
                None
            } else {
                Some(
                    info.child_gltf_indices
                        .iter()
                        .map(|index| json::Index::new(*index as u32))
                        .collect(),
                )
            },
            ..Default::default()
        });
    }

    log.push(format!(
        "Built {} nodes from prefab hierarchy",
        gltf_nodes.len()
    ));

    if !model.textures.is_empty() {
        samplers.push(json::texture::Sampler {
            mag_filter: Some(json::validation::Checked::Valid(
                json::texture::MagFilter::Linear,
            )),
            min_filter: Some(json::validation::Checked::Valid(
                json::texture::MinFilter::LinearMipmapLinear,
            )),
            wrap_s: json::validation::Checked::Valid(json::texture::WrappingMode::Repeat),
            wrap_t: json::validation::Checked::Valid(json::texture::WrappingMode::Repeat),
            name: None,
            extensions: None,
            extras: Default::default(),
        });
    }

    let mut texture_names: Vec<&String> = model.textures.keys().collect();
    texture_names.sort();

    for texture_name in texture_names {
        let (rgba_data, width, height) = &model.textures[texture_name];
        log.push(format!(
            "Embedding texture: {} ({}x{})",
            texture_name, width, height
        ));

        let png_data = {
            let mut png_buffer = Vec::new();
            let mut cursor = Cursor::new(&mut png_buffer);
            let encoder = image::codecs::png::PngEncoder::new(&mut cursor);
            image::ImageEncoder::write_image(
                encoder,
                rgba_data,
                *width,
                *height,
                image::ExtendedColorType::Rgba8,
            )?;
            png_buffer
        };

        while !buffer_data.len().is_multiple_of(4) {
            buffer_data.push(0);
        }

        let image_start = buffer_data.len();
        buffer_data.extend_from_slice(&png_data);
        let image_length = buffer_data.len() - image_start;

        buffer_views.push(json::buffer::View {
            buffer: json::Index::new(0),
            byte_offset: Some(USize64::from(image_start)),
            byte_length: USize64::from(image_length),
            byte_stride: None,
            target: None,
            name: None,
            extensions: None,
            extras: Default::default(),
        });

        let image_index = images.len() as u32;
        images.push(json::Image {
            buffer_view: Some(json::Index::new(buffer_views.len() as u32 - 1)),
            mime_type: Some(json::image::MimeType("image/png".to_string())),
            uri: None,
            name: Some(texture_name.clone()),
            extensions: None,
            extras: Default::default(),
        });

        let texture_index = gltf_textures.len() as u32;
        gltf_textures.push(json::Texture {
            sampler: Some(json::Index::new(0)),
            source: json::Index::new(image_index),
            name: Some(texture_name.clone()),
            extensions: None,
            extras: Default::default(),
        });

        texture_name_to_index.insert(texture_name.clone(), texture_index);
    }

    fn collect_mesh_materials(node: &PrefabNode, map: &mut HashMap<String, Material>) {
        if let Some(ref render_mesh) = node.components.render_mesh
            && let Some(ref material) = node.components.material
        {
            map.insert(render_mesh.name.clone(), material.clone());
        }
        for child in &node.children {
            collect_mesh_materials(child, map);
        }
    }

    let mut mesh_materials: HashMap<String, Material> = HashMap::new();
    for root_node in &model.prefab.root_nodes {
        collect_mesh_materials(root_node, &mut mesh_materials);
    }

    fn find_texture_by_keywords(
        texture_name_to_index: &HashMap<String, u32>,
        keywords: &[&str],
    ) -> Option<u32> {
        let mut matches: Vec<(&String, u32)> = texture_name_to_index
            .iter()
            .filter(|(name, _)| {
                let lower = name.to_lowercase();
                keywords.iter().any(|keyword| lower.contains(keyword))
            })
            .map(|(name, index)| (name, *index))
            .collect();
        matches.sort();
        matches.first().map(|(_, index)| *index)
    }

    log.push(format!(
        "Created {} textures, found {} mesh materials",
        gltf_textures.len(),
        mesh_materials.len()
    ));

    for (skin_index, skin) in model.skins.iter().enumerate() {
        log.push(format!(
            "Processing skin {} with {} joints",
            skin_index,
            skin.joints.len()
        ));

        let ibm_start = buffer_data.len();
        for ibm in &skin.inverse_bind_matrices {
            for col in 0..4 {
                for row in 0..4 {
                    let value = ibm[(row, col)];
                    let scaled_value = if col == 3 && row < 3 {
                        value * FBX_TO_GLTF_SCALE
                    } else {
                        value
                    };
                    buffer_data.extend_from_slice(&scaled_value.to_le_bytes());
                }
            }
        }
        let ibm_length = buffer_data.len() - ibm_start;

        buffer_views.push(json::buffer::View {
            buffer: json::Index::new(0),
            byte_offset: Some(USize64::from(ibm_start)),
            byte_length: USize64::from(ibm_length),
            byte_stride: None,
            target: None,
            name: None,
            extensions: None,
            extras: Default::default(),
        });

        let ibm_accessor_index = accessors.len() as u32;
        accessors.push(json::Accessor {
            buffer_view: Some(json::Index::new(buffer_views.len() as u32 - 1)),
            byte_offset: Some(USize64::from(0usize)),
            count: USize64::from(skin.inverse_bind_matrices.len()),
            component_type: json::validation::Checked::Valid(json::accessor::GenericComponentType(
                json::accessor::ComponentType::F32,
            )),
            type_: json::validation::Checked::Valid(json::accessor::Type::Mat4),
            min: None,
            max: None,
            name: None,
            normalized: false,
            sparse: None,
            extensions: None,
            extras: Default::default(),
        });

        let mut unmapped_joints = 0;
        let first_valid_node = node_index_map.values().next().copied().unwrap_or(0);

        let joint_indices: Vec<json::Index<json::Node>> = skin
            .joints
            .iter()
            .map(|&joint_index| {
                if let Some(&gltf_index) = node_index_map.get(&joint_index) {
                    json::Index::new(gltf_index as u32)
                } else {
                    unmapped_joints += 1;
                    json::Index::new(first_valid_node as u32)
                }
            })
            .collect();

        if unmapped_joints > 0 {
            log.push(format!(
                "  WARNING: {} of {} joints could not be mapped to glTF nodes",
                unmapped_joints,
                skin.joints.len()
            ));
        }

        skins.push(json::Skin {
            inverse_bind_matrices: Some(json::Index::new(ibm_accessor_index)),
            joints: joint_indices,
            skeleton: None,
            name: skin.name.clone(),
            extensions: None,
            extras: Default::default(),
        });
    }

    let mut mesh_names: Vec<&String> = model.meshes.keys().collect();
    mesh_names.sort();

    for mesh_name in mesh_names {
        let mesh = &model.meshes[mesh_name];
        let (vertices_to_use, skinned_data) = if let Some(ref skin_data) = mesh.skin_data {
            (None, Some(skin_data))
        } else {
            (Some(&mesh.vertices), None)
        };

        let vertex_count =
            skinned_data.map_or_else(|| mesh.vertices.len(), |data| data.skinned_vertices.len());

        log.push(format!(
            "Processing mesh: {} ({} vertices, {} indices, skinned: {})",
            mesh_name,
            vertex_count,
            mesh.indices.len(),
            skinned_data.is_some()
        ));

        let positions_start = buffer_data.len();
        let mut min_pos = [f32::MAX; 3];
        let mut max_pos = [f32::MIN; 3];

        if let Some(data) = skinned_data {
            for vertex in &data.skinned_vertices {
                let scaled_pos = [
                    vertex.position[0] * FBX_TO_GLTF_SCALE,
                    vertex.position[1] * FBX_TO_GLTF_SCALE,
                    vertex.position[2] * FBX_TO_GLTF_SCALE,
                ];
                buffer_data.extend_from_slice(&scaled_pos[0].to_le_bytes());
                buffer_data.extend_from_slice(&scaled_pos[1].to_le_bytes());
                buffer_data.extend_from_slice(&scaled_pos[2].to_le_bytes());
                for axis in 0..3 {
                    min_pos[axis] = min_pos[axis].min(scaled_pos[axis]);
                    max_pos[axis] = max_pos[axis].max(scaled_pos[axis]);
                }
            }
        } else if let Some(vertices) = vertices_to_use {
            for vertex in vertices {
                let scaled_pos = [
                    vertex.position[0] * FBX_TO_GLTF_SCALE,
                    vertex.position[1] * FBX_TO_GLTF_SCALE,
                    vertex.position[2] * FBX_TO_GLTF_SCALE,
                ];
                buffer_data.extend_from_slice(&scaled_pos[0].to_le_bytes());
                buffer_data.extend_from_slice(&scaled_pos[1].to_le_bytes());
                buffer_data.extend_from_slice(&scaled_pos[2].to_le_bytes());
                for axis in 0..3 {
                    min_pos[axis] = min_pos[axis].min(scaled_pos[axis]);
                    max_pos[axis] = max_pos[axis].max(scaled_pos[axis]);
                }
            }
        }
        let positions_length = buffer_data.len() - positions_start;

        buffer_views.push(json::buffer::View {
            buffer: json::Index::new(0),
            byte_offset: Some(USize64::from(positions_start)),
            byte_length: USize64::from(positions_length),
            byte_stride: None,
            target: Some(json::validation::Checked::Valid(
                json::buffer::Target::ArrayBuffer,
            )),
            name: None,
            extensions: None,
            extras: Default::default(),
        });

        let position_accessor_index = accessors.len() as u32;
        accessors.push(json::Accessor {
            buffer_view: Some(json::Index::new(buffer_views.len() as u32 - 1)),
            byte_offset: Some(USize64::from(0usize)),
            count: USize64::from(vertex_count),
            component_type: json::validation::Checked::Valid(json::accessor::GenericComponentType(
                json::accessor::ComponentType::F32,
            )),
            type_: json::validation::Checked::Valid(json::accessor::Type::Vec3),
            min: Some(json::Value::Array(
                min_pos
                    .iter()
                    .map(|value| json::Value::from(*value))
                    .collect(),
            )),
            max: Some(json::Value::Array(
                max_pos
                    .iter()
                    .map(|value| json::Value::from(*value))
                    .collect(),
            )),
            name: None,
            normalized: false,
            sparse: None,
            extensions: None,
            extras: Default::default(),
        });

        let normals_start = buffer_data.len();
        if let Some(data) = skinned_data {
            for vertex in &data.skinned_vertices {
                buffer_data.extend_from_slice(&vertex.normal[0].to_le_bytes());
                buffer_data.extend_from_slice(&vertex.normal[1].to_le_bytes());
                buffer_data.extend_from_slice(&vertex.normal[2].to_le_bytes());
            }
        } else if let Some(vertices) = vertices_to_use {
            for vertex in vertices {
                buffer_data.extend_from_slice(&vertex.normal[0].to_le_bytes());
                buffer_data.extend_from_slice(&vertex.normal[1].to_le_bytes());
                buffer_data.extend_from_slice(&vertex.normal[2].to_le_bytes());
            }
        }
        let normals_length = buffer_data.len() - normals_start;

        buffer_views.push(json::buffer::View {
            buffer: json::Index::new(0),
            byte_offset: Some(USize64::from(normals_start)),
            byte_length: USize64::from(normals_length),
            byte_stride: None,
            target: Some(json::validation::Checked::Valid(
                json::buffer::Target::ArrayBuffer,
            )),
            name: None,
            extensions: None,
            extras: Default::default(),
        });

        let normal_accessor_index = accessors.len() as u32;
        accessors.push(json::Accessor {
            buffer_view: Some(json::Index::new(buffer_views.len() as u32 - 1)),
            byte_offset: Some(USize64::from(0usize)),
            count: USize64::from(vertex_count),
            component_type: json::validation::Checked::Valid(json::accessor::GenericComponentType(
                json::accessor::ComponentType::F32,
            )),
            type_: json::validation::Checked::Valid(json::accessor::Type::Vec3),
            min: None,
            max: None,
            name: None,
            normalized: false,
            sparse: None,
            extensions: None,
            extras: Default::default(),
        });

        let texcoords_start = buffer_data.len();
        if let Some(data) = skinned_data {
            for vertex in &data.skinned_vertices {
                buffer_data.extend_from_slice(&vertex.tex_coords[0].to_le_bytes());
                buffer_data.extend_from_slice(&vertex.tex_coords[1].to_le_bytes());
            }
        } else if let Some(vertices) = vertices_to_use {
            for vertex in vertices {
                buffer_data.extend_from_slice(&vertex.tex_coords[0].to_le_bytes());
                buffer_data.extend_from_slice(&vertex.tex_coords[1].to_le_bytes());
            }
        }
        let texcoords_length = buffer_data.len() - texcoords_start;

        buffer_views.push(json::buffer::View {
            buffer: json::Index::new(0),
            byte_offset: Some(USize64::from(texcoords_start)),
            byte_length: USize64::from(texcoords_length),
            byte_stride: None,
            target: Some(json::validation::Checked::Valid(
                json::buffer::Target::ArrayBuffer,
            )),
            name: None,
            extensions: None,
            extras: Default::default(),
        });

        let texcoord_accessor_index = accessors.len() as u32;
        accessors.push(json::Accessor {
            buffer_view: Some(json::Index::new(buffer_views.len() as u32 - 1)),
            byte_offset: Some(USize64::from(0usize)),
            count: USize64::from(vertex_count),
            component_type: json::validation::Checked::Valid(json::accessor::GenericComponentType(
                json::accessor::ComponentType::F32,
            )),
            type_: json::validation::Checked::Valid(json::accessor::Type::Vec2),
            min: None,
            max: None,
            name: None,
            normalized: false,
            sparse: None,
            extensions: None,
            extras: Default::default(),
        });

        let mut joints_accessor_index = None;
        let mut weights_accessor_index = None;

        if let Some(data) = skinned_data {
            let joints_start = buffer_data.len();
            for vertex in &data.skinned_vertices {
                buffer_data.extend_from_slice(&(vertex.joint_indices[0] as u16).to_le_bytes());
                buffer_data.extend_from_slice(&(vertex.joint_indices[1] as u16).to_le_bytes());
                buffer_data.extend_from_slice(&(vertex.joint_indices[2] as u16).to_le_bytes());
                buffer_data.extend_from_slice(&(vertex.joint_indices[3] as u16).to_le_bytes());
            }
            let joints_length = buffer_data.len() - joints_start;

            buffer_views.push(json::buffer::View {
                buffer: json::Index::new(0),
                byte_offset: Some(USize64::from(joints_start)),
                byte_length: USize64::from(joints_length),
                byte_stride: None,
                target: Some(json::validation::Checked::Valid(
                    json::buffer::Target::ArrayBuffer,
                )),
                name: None,
                extensions: None,
                extras: Default::default(),
            });

            joints_accessor_index = Some(accessors.len() as u32);
            accessors.push(json::Accessor {
                buffer_view: Some(json::Index::new(buffer_views.len() as u32 - 1)),
                byte_offset: Some(USize64::from(0usize)),
                count: USize64::from(data.skinned_vertices.len()),
                component_type: json::validation::Checked::Valid(
                    json::accessor::GenericComponentType(json::accessor::ComponentType::U16),
                ),
                type_: json::validation::Checked::Valid(json::accessor::Type::Vec4),
                min: None,
                max: None,
                name: None,
                normalized: false,
                sparse: None,
                extensions: None,
                extras: Default::default(),
            });

            let weights_start = buffer_data.len();
            for vertex in &data.skinned_vertices {
                buffer_data.extend_from_slice(&vertex.joint_weights[0].to_le_bytes());
                buffer_data.extend_from_slice(&vertex.joint_weights[1].to_le_bytes());
                buffer_data.extend_from_slice(&vertex.joint_weights[2].to_le_bytes());
                buffer_data.extend_from_slice(&vertex.joint_weights[3].to_le_bytes());
            }
            let weights_length = buffer_data.len() - weights_start;

            buffer_views.push(json::buffer::View {
                buffer: json::Index::new(0),
                byte_offset: Some(USize64::from(weights_start)),
                byte_length: USize64::from(weights_length),
                byte_stride: None,
                target: Some(json::validation::Checked::Valid(
                    json::buffer::Target::ArrayBuffer,
                )),
                name: None,
                extensions: None,
                extras: Default::default(),
            });

            weights_accessor_index = Some(accessors.len() as u32);
            accessors.push(json::Accessor {
                buffer_view: Some(json::Index::new(buffer_views.len() as u32 - 1)),
                byte_offset: Some(USize64::from(0usize)),
                count: USize64::from(data.skinned_vertices.len()),
                component_type: json::validation::Checked::Valid(
                    json::accessor::GenericComponentType(json::accessor::ComponentType::F32),
                ),
                type_: json::validation::Checked::Valid(json::accessor::Type::Vec4),
                min: None,
                max: None,
                name: None,
                normalized: false,
                sparse: None,
                extensions: None,
                extras: Default::default(),
            });
        }

        let indices_start = buffer_data.len();
        for index in &mesh.indices {
            buffer_data.extend_from_slice(&(*index).to_le_bytes());
        }
        let indices_length = buffer_data.len() - indices_start;

        buffer_views.push(json::buffer::View {
            buffer: json::Index::new(0),
            byte_offset: Some(USize64::from(indices_start)),
            byte_length: USize64::from(indices_length),
            byte_stride: None,
            target: Some(json::validation::Checked::Valid(
                json::buffer::Target::ElementArrayBuffer,
            )),
            name: None,
            extensions: None,
            extras: Default::default(),
        });

        let indices_accessor_index = accessors.len() as u32;
        accessors.push(json::Accessor {
            buffer_view: Some(json::Index::new(buffer_views.len() as u32 - 1)),
            byte_offset: Some(USize64::from(0usize)),
            count: USize64::from(mesh.indices.len()),
            component_type: json::validation::Checked::Valid(json::accessor::GenericComponentType(
                json::accessor::ComponentType::U32,
            )),
            type_: json::validation::Checked::Valid(json::accessor::Type::Scalar),
            min: None,
            max: None,
            name: None,
            normalized: false,
            sparse: None,
            extensions: None,
            extras: Default::default(),
        });

        let mut attributes = std::collections::BTreeMap::new();
        attributes.insert(
            json::validation::Checked::Valid(json::mesh::Semantic::Positions),
            json::Index::new(position_accessor_index),
        );
        attributes.insert(
            json::validation::Checked::Valid(json::mesh::Semantic::Normals),
            json::Index::new(normal_accessor_index),
        );
        attributes.insert(
            json::validation::Checked::Valid(json::mesh::Semantic::TexCoords(0)),
            json::Index::new(texcoord_accessor_index),
        );

        if let Some(joints_index) = joints_accessor_index {
            attributes.insert(
                json::validation::Checked::Valid(json::mesh::Semantic::Joints(0)),
                json::Index::new(joints_index),
            );
        }
        if let Some(weights_index) = weights_accessor_index {
            attributes.insert(
                json::validation::Checked::Valid(json::mesh::Semantic::Weights(0)),
                json::Index::new(weights_index),
            );
        }

        let mesh_material = mesh_materials.get(mesh_name);
        let base_texture_index = mesh_material
            .and_then(|material| material.base_texture.as_ref())
            .and_then(|name| texture_name_to_index.get(name).copied())
            .or_else(|| {
                find_texture_by_keywords(
                    &texture_name_to_index,
                    &["diffuse", "albedo", "basecolor", "base_color"],
                )
            });
        let normal_texture_index = mesh_material
            .and_then(|material| material.normal_texture.as_ref())
            .and_then(|name| texture_name_to_index.get(name).copied())
            .or_else(|| find_texture_by_keywords(&texture_name_to_index, &["normal"]));
        let emissive_texture_index = mesh_material
            .and_then(|material| material.emissive_texture.as_ref())
            .and_then(|name| texture_name_to_index.get(name).copied());
        let metallic_roughness_texture_index = mesh_material
            .and_then(|material| material.metallic_roughness_texture.as_ref())
            .and_then(|name| texture_name_to_index.get(name).copied());

        let base_color_factor = if base_texture_index.is_some() {
            [1.0, 1.0, 1.0, 1.0]
        } else {
            mesh_material
                .map(|material| material.base_color)
                .unwrap_or([0.8, 0.8, 0.8, 1.0])
        };
        let roughness = mesh_material
            .map(|material| material.roughness)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        let metallic = mesh_material
            .map(|material| material.metallic)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let emissive_factor = mesh_material
            .map(|material| material.emissive_factor)
            .unwrap_or([0.0, 0.0, 0.0]);

        log.push(format!(
            "  Material: base texture {:?}, normal texture {:?}, roughness {:.2}, metallic {:.2}",
            base_texture_index, normal_texture_index, roughness, metallic
        ));

        let material_index = materials.len() as u32;
        materials.push(json::Material {
            name: Some(format!("{mesh_name}_material")),
            pbr_metallic_roughness: json::material::PbrMetallicRoughness {
                base_color_factor: json::material::PbrBaseColorFactor(base_color_factor),
                base_color_texture: base_texture_index.map(|index| json::texture::Info {
                    index: json::Index::new(index),
                    tex_coord: 0,
                    extensions: None,
                    extras: Default::default(),
                }),
                metallic_factor: json::material::StrengthFactor(metallic),
                roughness_factor: json::material::StrengthFactor(roughness),
                metallic_roughness_texture: metallic_roughness_texture_index.map(|index| {
                    json::texture::Info {
                        index: json::Index::new(index),
                        tex_coord: 0,
                        extensions: None,
                        extras: Default::default(),
                    }
                }),
                extensions: None,
                extras: Default::default(),
            },
            alpha_mode: json::validation::Checked::Valid(json::material::AlphaMode::Opaque),
            alpha_cutoff: None,
            double_sided: false,
            normal_texture: normal_texture_index.map(|index| json::material::NormalTexture {
                index: json::Index::new(index),
                scale: 1.0,
                tex_coord: 0,
                extensions: None,
                extras: Default::default(),
            }),
            occlusion_texture: None,
            emissive_texture: emissive_texture_index.map(|index| json::texture::Info {
                index: json::Index::new(index),
                tex_coord: 0,
                extensions: None,
                extras: Default::default(),
            }),
            emissive_factor: json::material::EmissiveFactor(emissive_factor),
            extensions: None,
            extras: Default::default(),
        });

        meshes.push(json::Mesh {
            primitives: vec![json::mesh::Primitive {
                attributes,
                indices: Some(json::Index::new(indices_accessor_index)),
                material: Some(json::Index::new(material_index)),
                mode: json::validation::Checked::Valid(json::mesh::Mode::Triangles),
                targets: None,
                extensions: None,
                extras: Default::default(),
            }],
            name: Some(mesh_name.clone()),
            weights: None,
            extensions: None,
            extras: Default::default(),
        });

        let skin_index = if skinned_data.is_some() {
            skinned_data.and_then(|data| data.skin_index).or(Some(0))
        } else {
            None
        };

        gltf_nodes.push(json::Node {
            mesh: Some(json::Index::new(meshes.len() as u32 - 1)),
            skin: skin_index.map(|index| json::Index::new(index as u32)),
            name: Some(mesh_name.clone()),
            ..Default::default()
        });
    }

    let mut gltf_animations: Vec<json::Animation> = Vec::new();

    let mut node_name_to_gltf_index: HashMap<String, usize> = HashMap::new();
    for (gltf_index, node) in gltf_nodes.iter().enumerate() {
        if let Some(ref name) = node.name {
            node_name_to_gltf_index.insert(name.clone(), gltf_index);
        }
    }

    for animation in animations {
        log.push(format!(
            "Processing animation: {} ({} channels)",
            animation.name,
            animation.channels.len()
        ));

        let mut animation_samplers: Vec<json::animation::Sampler> = Vec::new();
        let mut animation_channels: Vec<json::animation::Channel> = Vec::new();
        let mut unmapped_channels = 0;

        for channel in &animation.channels {
            let target_node_index = if let Some(ref target_name) = channel.target_bone_name {
                node_name_to_gltf_index.get(target_name).copied()
            } else {
                node_index_map.get(&channel.target_node).copied()
            };

            let Some(target_node_index) = target_node_index else {
                unmapped_channels += 1;
                continue;
            };

            let times_start = buffer_data.len();
            for time in &channel.sampler.input {
                buffer_data.extend_from_slice(&time.to_le_bytes());
            }
            let times_length = buffer_data.len() - times_start;

            buffer_views.push(json::buffer::View {
                buffer: json::Index::new(0),
                byte_offset: Some(USize64::from(times_start)),
                byte_length: USize64::from(times_length),
                byte_stride: None,
                target: None,
                name: None,
                extensions: None,
                extras: Default::default(),
            });

            let time_accessor_index = accessors.len() as u32;
            let min_time = channel.sampler.input.first().copied().unwrap_or(0.0);
            let max_time = channel.sampler.input.last().copied().unwrap_or(0.0);

            accessors.push(json::Accessor {
                buffer_view: Some(json::Index::new(buffer_views.len() as u32 - 1)),
                byte_offset: Some(USize64::from(0usize)),
                count: USize64::from(channel.sampler.input.len()),
                component_type: json::validation::Checked::Valid(
                    json::accessor::GenericComponentType(json::accessor::ComponentType::F32),
                ),
                type_: json::validation::Checked::Valid(json::accessor::Type::Scalar),
                min: Some(json::Value::Array(vec![json::Value::from(min_time)])),
                max: Some(json::Value::Array(vec![json::Value::from(max_time)])),
                name: None,
                normalized: false,
                sparse: None,
                extensions: None,
                extras: Default::default(),
            });

            let values_start = buffer_data.len();
            let (accessor_type, path, value_count) =
                match (&channel.target_property, &channel.sampler.output) {
                    (AnimationProperty::Translation, AnimationSamplerOutput::Vec3(values)) => {
                        for value in values {
                            buffer_data
                                .extend_from_slice(&(value.x * FBX_TO_GLTF_SCALE).to_le_bytes());
                            buffer_data
                                .extend_from_slice(&(value.y * FBX_TO_GLTF_SCALE).to_le_bytes());
                            buffer_data
                                .extend_from_slice(&(value.z * FBX_TO_GLTF_SCALE).to_le_bytes());
                        }
                        (
                            json::accessor::Type::Vec3,
                            json::animation::Property::Translation,
                            values.len(),
                        )
                    }
                    (AnimationProperty::Rotation, AnimationSamplerOutput::Quat(values)) => {
                        for value in values {
                            buffer_data.extend_from_slice(&value.i.to_le_bytes());
                            buffer_data.extend_from_slice(&value.j.to_le_bytes());
                            buffer_data.extend_from_slice(&value.k.to_le_bytes());
                            buffer_data.extend_from_slice(&value.w.to_le_bytes());
                        }
                        (
                            json::accessor::Type::Vec4,
                            json::animation::Property::Rotation,
                            values.len(),
                        )
                    }
                    (AnimationProperty::Scale, AnimationSamplerOutput::Vec3(values)) => {
                        for value in values {
                            buffer_data.extend_from_slice(&value.x.to_le_bytes());
                            buffer_data.extend_from_slice(&value.y.to_le_bytes());
                            buffer_data.extend_from_slice(&value.z.to_le_bytes());
                        }
                        (
                            json::accessor::Type::Vec3,
                            json::animation::Property::Scale,
                            values.len(),
                        )
                    }
                    _ => continue,
                };
            let values_length = buffer_data.len() - values_start;

            buffer_views.push(json::buffer::View {
                buffer: json::Index::new(0),
                byte_offset: Some(USize64::from(values_start)),
                byte_length: USize64::from(values_length),
                byte_stride: None,
                target: None,
                name: None,
                extensions: None,
                extras: Default::default(),
            });

            let value_accessor_index = accessors.len() as u32;
            accessors.push(json::Accessor {
                buffer_view: Some(json::Index::new(buffer_views.len() as u32 - 1)),
                byte_offset: Some(USize64::from(0usize)),
                count: USize64::from(value_count),
                component_type: json::validation::Checked::Valid(
                    json::accessor::GenericComponentType(json::accessor::ComponentType::F32),
                ),
                type_: json::validation::Checked::Valid(accessor_type),
                min: None,
                max: None,
                name: None,
                normalized: false,
                sparse: None,
                extensions: None,
                extras: Default::default(),
            });

            let sampler_index = animation_samplers.len();
            animation_samplers.push(json::animation::Sampler {
                input: json::Index::new(time_accessor_index),
                output: json::Index::new(value_accessor_index),
                interpolation: json::validation::Checked::Valid(
                    json::animation::Interpolation::Linear,
                ),
                extensions: None,
                extras: Default::default(),
            });

            animation_channels.push(json::animation::Channel {
                sampler: json::Index::new(sampler_index as u32),
                target: json::animation::Target {
                    node: json::Index::new(target_node_index as u32),
                    path: json::validation::Checked::Valid(path),
                    extensions: None,
                    extras: Default::default(),
                },
                extensions: None,
                extras: Default::default(),
            });
        }

        if unmapped_channels > 0 {
            log.push(format!(
                "  WARNING: {} channels could not be mapped to nodes",
                unmapped_channels
            ));
        }

        if !animation_channels.is_empty() {
            gltf_animations.push(json::Animation {
                name: Some(animation.name.clone()),
                channels: animation_channels,
                samplers: animation_samplers,
                extensions: None,
                extras: Default::default(),
            });
        }
    }

    while !buffer_data.len().is_multiple_of(4) {
        buffer_data.push(0);
    }

    let root_nodes: Vec<json::Index<json::Node>> = (0..model.prefab.root_nodes.len())
        .map(|index| json::Index::new(index as u32))
        .collect();

    let mesh_root_start = gltf_nodes.len() - model.meshes.len();
    let mesh_node_indices: Vec<json::Index<json::Node>> = (mesh_root_start..gltf_nodes.len())
        .map(|index| json::Index::new(index as u32))
        .collect();

    let all_root_nodes: Vec<json::Index<json::Node>> =
        root_nodes.into_iter().chain(mesh_node_indices).collect();

    let root = json::Root {
        asset: json::Asset {
            generator: Some("mixamo-to-gltf".to_string()),
            version: "2.0".to_string(),
            ..Default::default()
        },
        accessors,
        buffer_views,
        buffers: vec![json::Buffer {
            byte_length: USize64::from(buffer_data.len()),
            uri: None,
            name: None,
            extensions: None,
            extras: Default::default(),
        }],
        images,
        samplers,
        textures: gltf_textures,
        materials,
        meshes,
        nodes: gltf_nodes,
        skins,
        animations: gltf_animations,
        scenes: vec![json::Scene {
            nodes: all_root_nodes,
            name: Some("Scene".to_string()),
            extensions: None,
            extras: Default::default(),
        }],
        scene: Some(json::Index::new(0)),
        ..Default::default()
    };

    let json_string = json::serialize::to_string(&root)?;
    let json_bytes = json_string.as_bytes();

    let mut json_chunk = json_bytes.to_vec();
    while !json_chunk.len().is_multiple_of(4) {
        json_chunk.push(0x20);
    }

    let mut glb: Vec<u8> = Vec::new();

    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    let total_length = 12 + 8 + json_chunk.len() + 8 + buffer_data.len();
    glb.extend_from_slice(&(total_length as u32).to_le_bytes());

    glb.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
    glb.extend_from_slice(&json_chunk);

    glb.extend_from_slice(&(buffer_data.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x004E4942u32.to_le_bytes());
    glb.extend_from_slice(&buffer_data);

    log.push(format!(
        "GLB ready: {} bytes total, {} nodes, {} meshes, {} skins, {} animations, {} textures",
        glb.len(),
        root.nodes.len(),
        root.meshes.len(),
        root.skins.len(),
        root.animations.len(),
        root.textures.len()
    ));

    Ok(glb)
}
