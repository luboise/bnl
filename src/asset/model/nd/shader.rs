use super::prelude::*;

use indexmap::IndexMap;

use crate::d3d::{PixelShaderConstant, VertexShaderConstant};

#[derive(Debug, Clone, Serialize)]
pub struct NdShader2 {
    pub(crate) header: NdHeader,
}

impl NdNode for NdShader2 {
    fn header(&self) -> &NdHeader {
        &self.header
    }

    fn add_gltf_node(
        &self,
        _virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        let mesh_node_index = ctx
            .gltf
            .add_node(gltf::Node::new(Some("ndShader2".to_string())));

        Ok(Some(mesh_node_index))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NdShaderParam2 {
    pub(crate) header: NdHeader,

    pub(crate) main_payload: NdShaderParam2Payload,
    pub(crate) sub_payload: Option<NdShaderParam2Payload>,
}

impl NdNode for NdShaderParam2 {
    fn header(&self) -> &NdHeader {
        &self.header
    }

    fn add_gltf_node(
        &self,
        _virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        let main_attribute_map = self.main_payload().attribute_map();

        let attrib_key = "colour0";

        if let Some(attrib) = main_attribute_map.get(attrib_key) {
            main_attribute_map
                .get_index_of(attrib_key)
                .expect("Unable to find index for key that was literally just found.");

            let texture_slot = attrib.val2;

            match self
                .main_payload()
                .texture_assignments()
                .get(texture_slot as usize)
            {
                Some(tex_assignment) => {
                    let material_index = ctx.gltf.add_material(gltf::Material {
                        name: "Some Material".to_string(),
                        pbr_metallic_roughness: Some(gltf::PBRMetallicRoughness {
                            base_color_texture: Some(gltf::TextureInfo {
                                texture_index: tex_assignment.texture_index,
                                texcoords_accessor: None,
                            }),
                            metallic_factor: Some(0.0),
                            ..Default::default()
                        }),
                    });

                    ctx.current_material = Some(material_index);
                }
                None => eprintln!(
                    "Texture slot {} is referenced by an ndShaderParam, but the param only assigns {} slots.",
                    texture_slot + 1,
                    self.main_payload().texture_assignments().len()
                ),
            };
        }

        Ok(None)
    }
}

impl NdShaderParam2 {
    pub fn num_bound_textures(&self) -> usize {
        self.main_payload.texture_assignments.len()
    }

    pub fn main_payload(&self) -> &NdShaderParam2Payload {
        &self.main_payload
    }

    pub fn sub_payload(&self) -> Option<&NdShaderParam2Payload> {
        self.sub_payload.as_ref()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AttributeValue {
    pub(crate) val1: u32,
    pub(crate) val2: u32,

    pub(crate) sentinel1: u8,
    pub(crate) sentinel2: u8,
    pub(crate) sentinel3: u8,
    pub(crate) sentinel4: u8,
}

fn serialize_index_map<S>(
    index_map: &IndexMap<String, AttributeValue>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut map = serializer.serialize_map(None)?;

    for (key, value) in index_map {
        map.serialize_entry(&key, &value)?;
    }

    map.end()
}

pub const TEXTURE_ASSIGNMENT_SIZE: usize = 28;

#[derive(Debug, Clone, Serialize)]
pub struct TextureAssignment {
    pub(crate) texture_index: u32,
    pub(crate) count_1: u8,
    pub(crate) count_2: u8,
    pub(crate) count_3: u8,
    pub(crate) skip_diffuse_texture: bool,
    pub(crate) unknown_1: u32,
    pub(crate) unknown_2: u32,
    pub(crate) unknown_3: u32,
    pub(crate) unknown_4: u32,
    pub(crate) unknown_5: u32,
    // ORIGINAL FORMAT
    /*
       u32 textureIndex;
    u8 flag1;
    u8 flag2;
    u8 flag3;
    bool skipDiffuseTexture;
    u32 unknown3;
    u32 unknown4;
    u32 unknown5;
    u32 unknown6;
    u32 unknown7;
    */
}

impl TextureAssignment {
    fn from_model_slice(model_slice: ModelSlice) -> Result<Self, std::io::Error> {
        let mut cur = model_slice.new_cursor();

        let texture_index = cur.read_u32::<LittleEndian>()?;
        let count_1 = cur.read_u8()?;
        let count_2 = cur.read_u8()?;
        let count_3 = cur.read_u8()?;

        let skip_diffuse_texture: bool = !matches!(cur.read_u8()?, 0);

        let unknown_1 = cur.read_u32::<LittleEndian>()?;
        let unknown_2 = cur.read_u32::<LittleEndian>()?;
        let unknown_3 = cur.read_u32::<LittleEndian>()?;
        let unknown_4 = cur.read_u32::<LittleEndian>()?;
        let unknown_5 = cur.read_u32::<LittleEndian>()?;

        Ok(Self {
            texture_index,
            count_1,
            count_2,
            count_3,
            skip_diffuse_texture,
            unknown_1,
            unknown_2,
            unknown_3,
            unknown_4,
            unknown_5,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NdShaderParam2Payload {
    vertex_shader_constants: Vec<VertexShaderConstant>,
    pixel_shader_constants: Vec<[u8; 4]>,
    texture_assignments: Vec<TextureAssignment>,

    alpha_ref: u8, // Index to the alpha reference texture???
    count_1: u8,
    count_2: u8,
    some_count: u8,

    unknown_1: u32,
    next_payload: u32, // Pointer to next payload???

    #[serde(serialize_with = "serialize_index_map")]
    attribute_map: IndexMap<String, AttributeValue>,
    /*
    RawColour* pixelShaderConstants: u32 [[pointer_base("section1innersptr")]];
    u32* somePtr2: u32 [[pointer_base("section1innersptr")]];
    TextureAssignment* textureAssignments: u32 [[pointer_base("section1innersptr")]];
    u32 numTextureAssignments;
    u32 numBruhs;
    u32 numPixelShaderConstants;

    // 0x18
    u8 alphaReference;
    u8 flag1;
    u8 flag2;
    u8 someCount;

    u32 someU32_5;

    // 0x20
    u32* child: u32 [[pointer_base("section1innersptr")]];

    u32* assignmentsStart: u32 [[pointer_base("section1innersptr")]];
    u32 numAssignments;
        */
}

impl NdShaderParam2Payload {
    pub fn from_model_slice(model_slice: &ModelSlice) -> Result<Self, NdError> {
        let mut cur = Cursor::new(model_slice.slice);

        cur.seek(SeekFrom::Start(model_slice.read_start as u64))?;

        let pixel_shader_constants_start = cur.read_u32::<LittleEndian>()?;
        let vertex_shader_constants_start = cur.read_u32::<LittleEndian>()?;
        let texture_assignments_start = cur.read_u32::<LittleEndian>()?;
        let num_texture_assignments = cur.read_u32::<LittleEndian>()?;
        let num_vertex_shader_constants = cur.read_u32::<LittleEndian>()?;
        let num_pixel_shader_constants = cur.read_u32::<LittleEndian>()?;

        let alpha_ref = cur.read_u8()?;
        let count_1 = cur.read_u8()?;
        let count_2 = cur.read_u8()?;
        let some_count = cur.read_u8()?;

        let unknown_1 = cur.read_u32::<LittleEndian>()?;
        let next_payload_start = cur.read_u32::<LittleEndian>()?;
        let attributes_start = cur.read_u32::<LittleEndian>()?;
        let num_attributes = cur.read_u32::<LittleEndian>()?;

        let mut attribute_map = IndexMap::new();

        cur.seek(SeekFrom::Start(attributes_start as u64))?;

        for _ in 0..num_attributes {
            let name_ptr = cur.read_u32::<LittleEndian>()?;
            let val1 = cur.read_u32::<LittleEndian>()?;
            let val2 = cur.read_u32::<LittleEndian>()?;

            let sentinel1 = cur.read_u8()?;
            let sentinel2 = cur.read_u8()?;
            let sentinel3 = cur.read_u8()?;
            let sentinel4 = cur.read_u8()?;

            let mut name_cur = cur.clone();
            name_cur.seek(SeekFrom::Start(name_ptr as u64))?;

            let utf8_chars: Vec<u8> = name_cur
                .bytes()
                .map(|b| b.unwrap())
                .take_while(|b| *b != 0)
                .collect();

            let name = String::from_utf8(utf8_chars)
                .map_err(|e| NdError::CreationFailure(e.to_string()))?;

            if let Some(old_val) = attribute_map.insert(
                name.clone(),
                AttributeValue {
                    val1,
                    val2,
                    sentinel1,
                    sentinel2,
                    sentinel3,
                    sentinel4,
                },
            ) {
                println!(
                    "Overriding old entry in attribute map.\n{}: {:?}",
                    name, old_val
                );
            }
        }

        let vertex_constants_slice = &model_slice.slice[vertex_shader_constants_start as usize..];
        let vertex_shader_constants: Vec<VertexShaderConstant> = vertex_constants_slice
            .chunks_exact(size_of::<VertexShaderConstant>())
            .take(num_vertex_shader_constants as usize)
            .map(|chunk| {
                let mut constant: VertexShaderConstant = [0.0, 0.0, 0.0, 0.0];

                chunk.chunks_exact(4).enumerate().for_each(|(i, ch)| {
                    constant[i] = f32::from_le_bytes(ch.try_into().unwrap());
                });

                constant
            })
            .collect();

        let pixel_constants_slice = &model_slice.slice[pixel_shader_constants_start as usize..];
        let pixel_shader_constants: Vec<PixelShaderConstant> = pixel_constants_slice
            .chunks_exact(size_of::<PixelShaderConstant>())
            .take(num_pixel_shader_constants as usize)
            .map(|chunk| chunk.try_into().unwrap())
            .collect();

        let mut texture_assignments = vec![];

        for i in 0..num_texture_assignments as usize {
            texture_assignments.push(TextureAssignment::from_model_slice(
                model_slice.at(texture_assignments_start as usize + i * TEXTURE_ASSIGNMENT_SIZE),
            )?);
        }

        Ok(NdShaderParam2Payload {
            vertex_shader_constants,
            pixel_shader_constants,
            texture_assignments,
            alpha_ref,
            count_1,
            count_2,
            some_count,
            unknown_1,
            next_payload: next_payload_start,
            attribute_map,
        })
    }

    pub fn attribute_map(&self) -> &IndexMap<String, AttributeValue> {
        &self.attribute_map
    }

    pub fn texture_assignments(&self) -> &[TextureAssignment] {
        &self.texture_assignments
    }
}
