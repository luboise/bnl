use std::io::{Read, Seek, SeekFrom};

use super::prelude::*;

use indexmap::IndexMap;

use crate::d3d::VertexShaderConstant;

#[derive(Debug, Clone, serde::Serialize)]
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

#[derive(Debug, Clone, serde::Serialize)]
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

#[derive(Debug, Clone, serde::Serialize)]
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

    // u32* assignmentsStart: u32 [[pointer_base("section1innersptr")]];
    // u32 numAssignments;
    #[serde(serialize_with = "serialize_index_map")]
    attribute_map: IndexMap<String, AttributeValue>,
}

impl binrw::BinRead for NdShaderParam2Payload {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(
        _reader: &mut R,
        _endian: binrw::Endian,
        _args: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<Self> {
        todo!()
        /*
        let pixel_shader_constants_ptr = reader.read_u32::<LittleEndian>()?;
        let vertex_shader_constants_ptr = reader.read_u32::<LittleEndian>()?;
        let texture_assignments_ptr = reader.read_u32::<LittleEndian>()?;
        let num_texture_assignments = reader.read_u32::<LittleEndian>()?;
        let num_vertex_shader_constants = reader.read_u32::<LittleEndian>()?;
        let num_pixel_shader_constants = reader.read_u32::<LittleEndian>()?;

        let alpha_ref = reader.read_u8()?;
        let count_1 = reader.read_u8()?;
        let count_2 = reader.read_u8()?;
        let some_count = reader.read_u8()?;

        let unknown_1 = reader.read_u32::<LittleEndian>()?;
        let next_payload_start = reader.read_u32::<LittleEndian>()?;
        let attributes_start = reader.read_u32::<LittleEndian>()?;
        let num_attributes = reader.read_u32::<LittleEndian>()?;

        let mut attribute_map = IndexMap::new();

        reader.seek(SeekFrom::Start(attributes_start as u64))?;

        for _ in 0..num_attributes {
            let name_ptr = reader.read_u32::<LittleEndian>()?;
            let val1 = reader.read_u32::<LittleEndian>()?;
            let val2 = reader.read_u32::<LittleEndian>()?;

            let sentinel1 = reader.read_u8()?;
            let sentinel2 = reader.read_u8()?;
            let sentinel3 = reader.read_u8()?;
            let sentinel4 = reader.read_u8()?;

            let restore_pos = reader.stream_position()?;
            reader.seek(SeekFrom::Start(name_ptr as u64))?;

            let utf8_chars: Vec<u8> = reader
                .bytes()
                .map(|b| b.unwrap())
                .take_while(|b| *b != 0)
                .collect();

            let name = String::from_utf8(utf8_chars).map_err(|e| binrw::Error::Custom {
                pos: reader.stream_position().unwrap_or_default(),
                err: Box::new(e),
            })?;

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

        reader.seek(SeekFrom::Start(vertex_shader_constants_ptr.into()))?;
        let vertex_shader_constants = (0..num_vertex_shader_constants)
            .map(|_| reader.read_le())
            .collect::<Result<Vec<_>, _>>()?;

        reader.seek(SeekFrom::Start(pixel_shader_constants_ptr.into()))?;
        let pixel_shader_constants = (0..num_pixel_shader_constants)
            .map(|_| reader.read_le())
            .collect::<Result<Vec<_>, _>>()?;

        let mut texture_assignments = vec![];

        for i in 0..num_texture_assignments as usize {
            texture_assignments.push(TextureAssignment::from_model_slice(
                model_slice.at(texture_assignments_ptr as usize + i * TEXTURE_ASSIGNMENT_SIZE),
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
        */
    }
}

impl NdShaderParam2Payload {
    pub fn attribute_map(&self) -> &IndexMap<String, AttributeValue> {
        &self.attribute_map
    }

    pub fn texture_assignments(&self) -> &[TextureAssignment] {
        &self.texture_assignments
    }
}

#[derive(Debug, Clone)]
pub struct NdShaderParam2Data {
    main_payload: NdShaderParam2Payload,
    sub_payload: Option<NdShaderParam2Payload>,
}

#[binrw::binrw]
#[derive(Debug, Clone)]
struct VertexShader {
    idk1: u16,
    num_dwords: u16,

    #[br(count = 4 * num_dwords * 4)]
    data: Vec<u8>,
}

impl VertexShader {
    pub fn byte_len(&self) -> usize {
        4 + self.data.len()
    }
}

#[binrw::binrw]
#[br(little)]
#[bw(little)]
#[derive(Debug, Clone)]
struct OtherStream {
    #[br(
        assert(!stream.is_empty() && *stream.last().unwrap() == 0xffffffff),
        parse_with = binrw::helpers::until(|byte| *byte == 0xffffffff),
    )]
    stream: Vec<u32>,
}

impl OtherStream {
    pub fn byte_len(&self) -> usize {
        self.stream.len() * 4
    }
}

#[binrw::binrw]
#[bw(stream = w)]
#[derive(Debug, Clone)]
pub struct NdVertexShaderData {
    #[br(temp)]
    #[bw(try_calc = u32::try_from(w.stream_position()?
        + (0x58)
        + u64::try_from(
            other_stream.byte_len() 
             + vertex_shader.byte_len() 
            ).map_err(super::br_error(w))?
        ))]
    stream_source_indices_ptr: u32,
    #[br(temp)]
    #[bw(try_calc = u32::try_from(stream_source_indices.len()))]
    num_stream_sources: u32,
    constant1: u32,
    constant2: u16,
    idk1: u16,
    // 0x10
    idk2: u32,
    idk3: u32,
    idk4: u32,
    count1: u32,
    // 0x20
    count2: u32,
    count3: u32,
    idk_2_1: u32,
    idk_2_2: u32,
    // 0x30
    #[br(temp)]
    #[bw(try_calc = u32::try_from(w.stream_position()?
        + (0x58 - 0x30)))]
    other_stream_ptr: u32,
    #[br(temp)]
    #[bw(try_calc = u32::try_from(w.stream_position()?
        + (0x58 - 0x34)
        + u64::try_from(other_stream.byte_len()).map_err(super::br_error(w))?
        ))]
    vertex_shader_ptr: u32,
    #[br(temp)]
    #[bw(try_calc = u32::try_from(w.stream_position()?
        + (0x58 - 0x38)
        + u64::try_from(
            vertex_shader.byte_len() 
             + other_stream.byte_len() 
             + stream_source_indices.len() * 4
            ).map_err(super::br_error(w))?))]
    strides_ptr: u32,
    #[brw(magic = b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00ndVertexShader\x00\x00")]
    _name: (),
    #[br(seek_before = SeekFrom::Start(other_stream_ptr.into()))]
    other_stream: OtherStream,
    #[br(seek_before=SeekFrom::Start(vertex_shader_ptr.into()))]
    vertex_shader: VertexShader,
    #[br(count = num_stream_sources,
        seek_before = SeekFrom::Start(stream_source_indices_ptr.into()))]
    stream_source_indices: Vec<u32>,
    #[br(count = num_stream_sources,
        seek_before = SeekFrom::Start(strides_ptr.into()))]
    strides: Vec<u8>,
}
