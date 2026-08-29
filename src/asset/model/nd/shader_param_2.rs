use std::io::SeekFrom;

use binrw::{BinReaderExt, BinWriterExt};
use serde::ser::SerializeMap;

use crate::asset::model::nd::{br_error, br_get_stream_pos};

#[binrw::binrw]
#[bw(stream = w)]
#[br(stream = r)]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NdShaderParam2Data {
    #[br(temp)]
    #[bw(try_calc = u32::try_from(8 + w.stream_position().unwrap()))]
    payload_ptr: u32,
    #[br(temp, assert(payload_ptr == r.stream_position().unwrap() as u32))]
    #[bw(try_calc = 
        if sub_payload.is_some() {
            u32::try_from(4 + w.stream_position().unwrap() + main_payload.size() as u64)
        }
        else {
            Ok(0)
        })]
    payload_ptr_2: u32,
    pub main_payload: PixelShaderParams,
    #[br(if(payload_ptr_2 != 0),
        seek_before = SeekFrom::Start(payload_ptr_2.into()))]
    pub sub_payload: Option<PixelShaderParams>,
    #[brw(magic = b"ndShaderParam2\x00\x00")]
    _name: (),
}

impl NdShaderParam2Data {
    pub fn name_offset(&self) -> i64 {
        8 + self.main_payload.size() + self.sub_payload.as_ref().map(|payload|payload.size()).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(transparent)]
pub struct MaybeIndex(pub Option<u32>);

impl From<u32> for MaybeIndex {
    fn from(value: u32) -> Self {
        match value {
            0 => Self(None),
            x => Self(Some(x - 1))
        }
    }
}

impl From<MaybeIndex> for u32 {
    fn from(value: MaybeIndex) -> Self {
        match value.0 {
            Some(x) => x + 1,
            None => 0
        }
    }
}

impl binrw::BinRead for MaybeIndex {
    type Args<'a> = ();
    fn read_options<R: std::io::prelude::Read + std::io::prelude::Seek>(
        reader: &mut R,
        _: binrw::Endian,
        _: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<Self> {
        Ok(reader.read_le::<u32>()?.into())
    }
}

impl binrw::BinWrite for MaybeIndex {
    type Args<'a> = ();
    fn write_options<W: std::io::prelude::Write + std::io::prelude::Seek>(
        &self,
        writer: &mut W,
        _: binrw::Endian,
        _: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<()> {
        writer.write_le(&u32::from(*self))?;
        Ok(())
    }
}

#[binrw::binread]
#[derive(Debug, Clone, serde::Serialize)]
#[br(import(num_assignments: u32))]
pub struct ParamAssignment {
    #[br(temp)]
    name_ptr: u32,
    #[br(seek_before = SeekFrom::Start(name_ptr.into()),
        restore_position)]
    #[serde(serialize_with = "super::serialize_nullstring")]
    pub name: binrw::NullString,

    // 0x4
    pub idk1: u32,

    pub texture_assignment_index: MaybeIndex,
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

#[derive(Debug, Clone, Default, serde::Serialize)]
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
            let ParamAssignment { name, idk1, texture_assignment_index, colour } = param_assignment;
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
            texture_assignment_index.write_le(writer)?;
            colour.write_le(writer)?;
        }

        name_buf.write_le(writer)?;

        Ok(())
    }
}


#[wezat::wz]
#[derive(Clone, Debug, serde::Serialize)]
pub struct PixelShaderMatrix {
    #[serde(skip)]
    matrix: [[f32; 4]; 4],
    val1: u32,
    val2: u32,
}

#[wezat::wz]
#[derive(Debug, Clone, serde::Serialize)]
pub struct PixelShaderParams {
    pixel_shader_constants_ptr: &pixel_shader_constants,
    matrices_ptr: &matrices,
    texture_assignments_ptr: &texture_assignments,
    num_texture_assignments: u32,
    num_matrices: u32,
    num_pixel_shader_constants: u32,

    alpha_ref: u8, // Index to the alpha reference texture???
    count_1: u8,
    count_2: u8,
    some_count: u8,

    unknown_1: u32,

    // 0x20
    next_payload_ptr: u32, // Pointer to next payload???

    param_assignments_ptr: &param_assignments,
    num_param_assignments: u32,

    pub idk1: u32,
    pub idk2: u32,
    pub  idk3: u32,
     pub idk4: u32,
     pub idk5: u32,

     pub weird_tex_index: u32,
     pub idk6: u32,

    pub pixel_shader_constants: [[u8; 4]; num_pixel_shader_constants],
    pub matrices: [PixelShaderMatrix; num_matrices],
    pub texture_assignments: [TextureAssignment; num_texture_assignments],
    pub param_assignments: ParamAssignments,
}

impl PixelShaderParams {
    pub fn size(&self) -> i64 {
        let v = 0x48
            + 4 * self.pixel_shader_constants.len()
            + 4 * 16 * self.matrices.len() + if self.matrices.is_empty()  { 0 } else { 8 }
            + 4 * 7 * self.texture_assignments.len();

        let param_assignments_size = self.param_assignments.assignments.iter()
            .map(|param_assignment| param_assignment.key_len() + 16)
            .sum::<i64>();

        v as i64 + param_assignments_size
    }

    #[deprecated(note = "Access the field instead, PixelShaderParams.texture_assignments")]
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
    pub(crate) texture_index: MaybeIndex,
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

// TODO: Move this into wezat as a unit struct derive instead
impl wezat::Wezat for TextureAssignment {
    const MIN_SIZE: usize = 0;

    fn from_bytes(reader: &mut impl wezat::Reader) -> Result<Self, wezat::Error> {
        let texture_index = wezat::read::<u32>(reader)?.into();
        let count_1 = wezat::read(reader)?;
        let count_2 = wezat::read(reader)?;
        let count_3 = wezat::read(reader)?;
        let skip_diffuse_texture = wezat::read(reader)?;
        let unknown_1 = wezat::read(reader)?;
        let unknown_2 = wezat::read(reader)?;
        let unknown_3 = wezat::read(reader)?;
        let unknown_4 = wezat::read(reader)?;
        let unknown_5 = wezat::read(reader)?;

        Ok(Self{ texture_index, count_1, count_2, count_3, skip_diffuse_texture, unknown_1, unknown_2, unknown_3, unknown_4, unknown_5 })
    }

    fn write_bytes(&self, writer: &mut impl wezat::Writer) -> Result<(), wezat::Error> {
        let Self{ texture_index, count_1, count_2, count_3, skip_diffuse_texture, unknown_1, unknown_2, unknown_3, unknown_4, unknown_5 } = self;
        u32::from(*texture_index).write_bytes(writer)?;
        count_1.write_bytes(writer)?;
        count_2.write_bytes(writer)?;
        count_3.write_bytes(writer)?;
        skip_diffuse_texture.write_bytes(writer)?;
        unknown_1.write_bytes(writer)?;
        unknown_2.write_bytes(writer)?;
        unknown_3.write_bytes(writer)?;
        unknown_4.write_bytes(writer)?;
        unknown_5.write_bytes(writer)?;

        Ok(())
    }
}
