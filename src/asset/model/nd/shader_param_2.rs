use std::io::SeekFrom;

use binrw::{BinReaderExt, BinWrite, BinWriterExt};
use serde::ser::SerializeMap;

use crate::asset::model::nd::{br_error, br_get_stream_pos};

#[binrw::binrw]
#[bw(stream = w)]
#[br(stream = r)]
#[derive(Debug, Clone)]
pub struct NdShaderParam2Data {
    #[br(temp)]
    #[bw(try_calc = u32::try_from(8 + w.stream_position().unwrap()))]
    payload_ptr: u32,
    #[br(temp, assert(payload_ptr == r.stream_position().unwrap() as u32))]
    #[bw(calc = 0)]
    payload_ptr_2: u32,
    main_payload: PixelShaderParams,
    #[br(if(payload_ptr_2 != 0),
        seek_before = SeekFrom::Start(payload_ptr_2.into()))]
    sub_payload: Option<PixelShaderParams>,
    #[brw(magic = b"ndShaderParam2\x00\x00")]
    _name: (),
}

impl NdShaderParam2Data {
    pub fn name_offset(&self) -> i64 {
        8 + self.main_payload.size() + self.sub_payload.as_ref().map(|payload|payload.size()).unwrap_or(0)
    }
}

#[binrw::binread]
#[derive(Debug, Clone)]
#[br(import(num_assignments: u32))]
pub struct ParamAssignment {
    #[br(temp)]
    name_ptr: u32,
    #[br(seek_before = SeekFrom::Start(name_ptr.into()),
        restore_position)]
    pub name: binrw::NullString,

    // 0x4
    pub idk1: u32,
    pub texture_index: u32,
    #[br(assert(colour == 0xffffffff))]
    pub colour: u32,
}

impl ParamAssignment {
    pub fn key_len(&self) -> i64 {
        let mut len = 1 + self.name.0.len() as i64;
        while len % 4 != 0 {
            len += 1;
        }
        len
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParamAssignments {
    pub assignments: Vec<ParamAssignment>
}

impl ParamAssignments {
    pub fn len(&self) -> usize {
        self.assignments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl binrw::BinRead for ParamAssignments {
    type Args<'a> = u32;

    fn read_options<R: std::io::prelude::Read + std::io::prelude::Seek>(
        reader: &mut R,
        _: binrw::Endian,
        num_assignments: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<Self> {
        let mut assignments = vec![];

        let stream_pos = reader.stream_position()?;

        for i in 0..num_assignments as u64 {
            reader.seek(SeekFrom::Start(stream_pos + i * 16))?;
            assignments.push(reader.read_le::<ParamAssignment>()?);
        }


        let offset = assignments.iter().map(|assignment| {
            let len = assignment.name.0.len() + 1;

            if len % 4 == 0 {
                len 
            }
            else {
                len + 4 - (len % 4) 
            }
            }).sum::<usize>() as i64;

        reader.seek_relative(offset)?;
        Ok(Self { assignments })
    }
}

impl binrw::BinWrite for ParamAssignments {
    type Args<'a> = ();

    fn write_options<W: std::io::prelude::Write + std::io::prelude::Seek>(
        &self,
        writer: &mut W,
        _: binrw::Endian,
        (): Self::Args<'_>,
    ) -> binrw::prelude::BinResult<()> {
        if self.is_empty() {
            return Ok(())
        }

        let param_assignments = &self.assignments;

        let base = u32::try_from(writer.stream_position()?).map_err(br_error(writer))?;
        let mut name_ptr = base + 16 * u32::try_from(param_assignments.len()).map_err(br_error(writer))?;

        let mut name_buf = vec![];

        for param_assignment in param_assignments {
            let ParamAssignment { name, idk1, texture_index, colour } = param_assignment;
            writer.write_le(&name_ptr)?;
            let mut name_bytes = name.0.clone();
            // null char
            name_bytes.push(0);
            while name_bytes.len() % 4 != 0 {
                name_bytes.push(0);
            }
            name_ptr += u32::try_from(name_bytes.len()).map_err(br_error(writer))?;
            name_buf.extend(name_bytes);

            idk1.write_le(writer)?;
            texture_index.write_le(writer)?;
            colour.write_le(writer)?;
        }

        name_buf.write_le(writer)?;

        Ok(())
    }
}

#[binrw::binrw]
#[bw(stream = w)]
#[derive(Debug, Clone)]
pub struct PixelShaderParams {
    #[br(temp)]
    #[bw(try_calc = if pixel_shader_constants.is_empty() {Ok(0)} else {
            br_get_stream_pos(w, 0).map(|v| v + 0x48)
        })]
    pixel_shader_constants_ptr: u32,
    #[br(temp)]
    #[bw(try_calc = if matrices.is_empty() {Ok(0)} else {
            br_get_stream_pos(w, 4).map(|v| v + 0x48 
                + u32::try_from(pixel_shader_constants.len()).unwrap() * 4)
        })]
    matrices_ptr: u32,
    #[br(temp)]
    #[bw(try_calc = if texture_assignments.is_empty() {Ok(0)} else {
            br_get_stream_pos(w, 8)
                .map(|v| (v as usize + 0x48
                + 4 * pixel_shader_constants.len()
                + 4 * 16 * matrices.len() + if matrices.is_empty() {0} else {8}) 
                as u32)})]
    texture_assignments_ptr: u32,
    #[br(temp)]
    #[bw(try_calc = texture_assignments.len().try_into())]
    num_texture_assignments: u32,
    #[br(temp)]
    #[bw(try_calc = matrices.len().try_into())]
    num_matrices: u32,
    #[br(temp)]
    #[bw(try_calc = pixel_shader_constants.len().try_into())]
    num_pixel_shader_constants: u32,

    alpha_ref: u8, // Index to the alpha reference texture???
    count_1: u8,
    count_2: u8,
    some_count: u8,

    unknown_1: u32,

    // 0x20
    #[brw(magic = 0u32)]
    _next_payload_magic: (), // Pointer to next payload???

    #[br(temp)]
    #[bw(
        // if(param_assignments.len() > 0),
        try_calc = if param_assignments.is_empty() {Ok(0)} else {
            br_get_stream_pos(w, 0x24)
                .map(|v| (v as usize + 0x48
                + 4 * pixel_shader_constants.len()
                + 4 * 16 * matrices.len() + if matrices.is_empty() {0} else {8}
                + 4 * 7 * texture_assignments.len()
                ) 
                as u32)})]
    param_assignments_ptr: u32,
    #[br(temp)]
    #[bw(try_calc = param_assignments.len().try_into())]
    num_param_assignments: u32,

 pub    idk1: u32,
    pub idk2: u32,
 pub    idk3: u32,
     pub idk4: u32,
     pub idk5: u32,

     pub weird_tex_index: u32,
     pub idk6: u32,

    #[br(if(pixel_shader_constants_ptr != 0),
        count = num_pixel_shader_constants)]
    pub pixel_shader_constants: Vec<[u8; 4]>,

    #[br(if(matrices_ptr != 0),
        count = num_matrices)]
    pub matrices: Vec<[[f32; 4]; 4]>,

    #[brw(if(!matrices.is_empty()), magic = b"\x07\x00\x00\x00\x01\x00\x00\x00")]
    _extra_vals_magic: (),

    #[br(if(texture_assignments_ptr != 0 && num_texture_assignments > 0),
        count = num_texture_assignments)]
    pub texture_assignments: Vec<TextureAssignment>,

    #[br(if(num_param_assignments > 0 && param_assignments_ptr > 0),
        args_raw(num_param_assignments))]
    pub param_assignments: ParamAssignments,
}


impl PixelShaderParams {
    pub fn size(&self) -> i64 {
        let v =0x48
            + 4 * self.pixel_shader_constants.len()
            + 4 * 16 * self.matrices.len() + if self.matrices.is_empty()  { 0 } else { 8 }
            + 4 * 7 * self.texture_assignments.len();

        let param_assignments_size = self.param_assignments.assignments.iter()
            .map(|param_assignment| param_assignment.key_len() + 16)
            .sum::<i64>();

        let v = v as i64 + param_assignments_size;

        assert_eq!(v, 496);

        v
    }

    pub fn attribute_map(&self) -> &indexmap::IndexMap<String, AttributeValue> {
        todo!()
        // &self.attribute_map
    }

    pub fn texture_assignments(&self) -> &[TextureAssignment] {
        &self.texture_assignments
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AttributeValue {
    pub val1: u32,
    pub val2: u32,

    pub sentinel1: u8,
    pub sentinel2: u8,
    pub sentinel3: u8,
    pub sentinel4: u8,
}

fn serialize_index_map<S>(
    index_map: &indexmap::IndexMap<String, AttributeValue>,
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

#[binrw::binrw]
#[derive(Debug, Clone, serde::Serialize)]
pub struct TextureAssignment {
    pub(crate) texture_index: u32,
    pub(crate) count_1: u8,
    pub(crate) count_2: u8,
    pub(crate) count_3: u8,
    pub(crate) skip_diffuse_texture: u8, // realistically a bool?
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


