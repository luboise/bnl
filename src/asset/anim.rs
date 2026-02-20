use std::io::{Cursor, Read};

use byteorder::{LittleEndian, ReadBytesExt};
use gltf_writer::gltf::{NodeTransform, Quaternion};

use crate::{
    VirtualResource,
    asset::{AssetDescriptor, AssetLike, AssetParseError, AssetType},
    utils::bitstream::BitStream,
};

#[derive(Debug, Clone, PartialEq)]
pub enum AnimValueUsageType {
    Interpolated,
    Raw,
    Unused,
}

#[derive(Debug, Clone)]
pub struct Vec3UsageType {
    x: AnimValueUsageType,
    y: AnimValueUsageType,
    z: AnimValueUsageType,
    bit1: bool,
    bit0: bool,
}

impl From<u8> for Vec3UsageType {
    fn from(value: u8) -> Self {
        let usages = (0..3)
            .map(|i| {
                if (value << i) & 0b10000000 > 0 {
                    if value & 0b00010000 > 0 {
                        AnimValueUsageType::Interpolated
                    } else {
                        AnimValueUsageType::Raw
                    }
                } else {
                    AnimValueUsageType::Unused
                }
            })
            .collect::<Vec<_>>();

        let [x, y, z] = usages.try_into().unwrap();

        Self {
            x,
            y,
            z,
            bit1: value & 0b00000010 == 2,
            bit0: value & 1 == 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PrecisionSpecifiers {
    unknown_u8: u8,
    /// u5
    scale_divisor: u8,
    /// u5
    unknown_u5: u8,
    /// u5
    pos_divisor: u8,
    /// u5
    quat_divisor: u8,
    /// u4
    unknown_u4: u8,
}

impl PrecisionSpecifiers {
    pub fn scale_divisor(&self) -> u32 {
        // 2^n
        2u32.pow(self.scale_divisor.into())
    }
    #[inline]
    pub fn scale_constant(&self) -> f32 {
        (self.scale_divisor() as f32).recip()
    }

    pub fn pos_divisor(&self) -> u32 {
        // 2^n
        2u32.pow(self.pos_divisor.into())
    }
    #[inline]
    pub fn pos_constant(&self) -> f32 {
        (self.pos_divisor() as f32).recip()
    }

    pub fn quat_divisor(&self) -> u32 {
        // 2^(n-1) - 1
        2u32.pow(self.quat_divisor as u32 - 1) - 1
    }
    #[inline]
    pub fn quat_constant(&self) -> f32 {
        (self.quat_divisor() as f32).recip()
    }
}

impl From<u32> for PrecisionSpecifiers {
    fn from(value: u32) -> Self {
        Self {
            unknown_u8: (value & 0b11111111) as u8,
            scale_divisor: ((value >> 8) & 0b11111) as u8,
            unknown_u5: ((value >> (8 + 5)) & 0b11111) as u8,
            pos_divisor: ((value >> (8 + 5 + 5)) & 0b11111) as u8,
            quat_divisor: ((value >> (8 + 5 + 5 + 5)) & 0b11111) as u8,
            unknown_u4: ((value >> (8 + 5 + 5 + 5 + 5)) & 0b1111) as u8,
        }
    }
}

impl From<PrecisionSpecifiers> for u32 {
    fn from(value: PrecisionSpecifiers) -> Self {
        (value.unknown_u8 as u32)
            | ((value.scale_divisor as u32) << 8)
            | ((value.unknown_u5 as u32) << (8 + 5))
            | ((value.pos_divisor as u32) << (8 + 5 + 5))
            | ((value.quat_divisor as u32) << (8 + 5 + 5 + 5))
            | ((value.unknown_u4 as u32) << (8 + 5 + 5 + 5 + 5))
    }
}

#[derive(Debug, Clone)]
pub struct PackFormat {
    qx: AnimValueUsageType,
    qy: AnimValueUsageType,
    qz: AnimValueUsageType,

    translation: Option<Vec3UsageType>,
    scale: Option<Vec3UsageType>,
}

impl PackFormat {
    pub fn from_mut_cursor(cur: &mut Cursor<&[u8]>) -> Result<Self, AssetParseError> {
        let q_format = cur.read_u8()?;

        let usages = (0..3)
            .map(|i| {
                if (q_format << i) & 0b10000000 > 0 {
                    if q_format & 0b00010000 > 0 {
                        AnimValueUsageType::Interpolated
                    } else {
                        AnimValueUsageType::Raw
                    }
                } else {
                    AnimValueUsageType::Unused
                }
            })
            .collect::<Vec<_>>();

        let [qx, qy, qz] = usages.try_into().unwrap();

        let translation = if q_format & 0b10 == 0b10 {
            Some(Vec3UsageType::from(cur.read_u8()?))
        } else {
            None
        };

        let scale = if q_format & 0b01 == 0b01 {
            Some(Vec3UsageType::from(cur.read_u8()?))
        } else {
            None
        };

        Ok(Self {
            qx,
            qy,
            qz,
            translation,
            scale,
        })
    }
}

#[derive(Clone)]
pub struct AnimDescriptor {
    magic: [u8; 4],
    inverse_divisor: f32,
    duration: f32,
    c_vals_ptr: u32,
    // 0x10
    some_ptr_1: u32,
    // 0x14
    num_bones: u16,
    unused_1: u16,
    // 0x18
    num_keyframes: u16,
    unused_2: u16,
    // 0x1c
    precision_specifiers: PrecisionSpecifiers,
    // 0x20
    some_ptr_2: u32,
    some_u32_1: u32,
    tail_data_ptr: u32,
    some_u32_2: u32,
    // 0x30
    some_u32_3: u32,
    some_u32_4: u32,
    some_u32_5: u32,
    some_u32_6: u32,

    header_size: u16,
    section1_size: u16,
    section2_size: u16,

    // Possibly number of floats per keyframe?
    // This maps to KfTransform with (quat, vec3, vec3), which is 10 floats, and
    // aid_anim_canbounce has 10 as this value
    keyframe_size: u16,
    some_float: f32,

    pack_formats: Vec<PackFormat>,

    shorts: Vec<i16>,

    bits_per_channel: Vec<u8>,
    keyframe_bytes: Vec<u8>,
}

impl AnimDescriptor {
    #[inline]
    pub fn bits_per_keyframe_exact(&self) -> usize {
        self.bits_per_channel.iter().map(|v| *v as usize).sum()
    }

    pub fn inverse_divisor(&self) -> f32 {
        self.inverse_divisor
    }

    pub fn duration(&self) -> f32 {
        self.duration
    }

    pub fn precision_specifiers(&self) -> &PrecisionSpecifiers {
        &self.precision_specifiers
    }

    pub fn pack_formats(&self) -> &[PackFormat] {
        &self.pack_formats
    }

    pub fn shorts(&self) -> &[i16] {
        &self.shorts
    }

    pub fn bits_per_channel(&self) -> &[u8] {
        &self.bits_per_channel
    }

    pub fn transforms_per_keyframe(&self) -> u16 {
        self.num_bones
    }

    pub fn num_keyframes(&self) -> u16 {
        self.num_keyframes
    }
}

impl std::fmt::Debug for AnimDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnimDescriptor")
            .field("magic", &self.magic)
            .field("inverse_divisor", &self.inverse_divisor)
            .field("duration", &self.duration)
            .field("c_vals_ptr", &self.c_vals_ptr)
            .field("some_ptr_1", &self.some_ptr_1)
            .field("transforms_per_keyframe", &self.num_bones)
            .field("unused_1", &self.unused_1)
            .field("num_keyframes", &self.num_keyframes)
            .field("unused_2", &self.unused_2)
            .field("precision_specifiers", &self.precision_specifiers)
            .field("some_ptr_2", &self.some_ptr_2)
            .field("some_u32_1", &self.some_u32_1)
            .field("tail_data_ptr", &self.tail_data_ptr)
            .field("some_u32_2", &self.some_u32_2)
            .field("some_u32_3", &self.some_u32_3)
            .field("some_u32_4", &self.some_u32_4)
            .field("some_u32_5", &self.some_u32_5)
            .field("some_u32_6", &self.some_u32_6)
            .field("header_size", &self.header_size)
            .field("section1_size", &self.section1_size)
            .field("section2_size", &self.section2_size)
            .field("keyframe_size", &self.keyframe_size)
            .field("some_float", &self.some_float)
            .field("pack_formats", &self.pack_formats)
            .field("shorts", &self.shorts)
            .field("bits_per_channel", &self.bits_per_channel)
            .finish()
    }
}

#[derive(Debug, Default, Clone)]
pub struct PartialTransform {
    // Quaternion Rotation
    qx: Option<f32>,
    qy: Option<f32>,
    qz: Option<f32>,
    // Translation
    tx: Option<f32>,
    ty: Option<f32>,
    tz: Option<f32>,
    // Scale
    sx: Option<f32>,
    sy: Option<f32>,
    sz: Option<f32>,
}

impl From<PartialTransform> for NodeTransform {
    fn from(value: PartialTransform) -> Self {
        NodeTransform::TRS(
            [
                value.tx.unwrap_or(0.0),
                value.ty.unwrap_or(0.0),
                value.tz.unwrap_or(0.0),
            ],
            // TODO: Quaternion math
            [0.0, 0.0, 0.0],
            [
                value.sx.unwrap_or(1.0),
                value.sy.unwrap_or(1.0),
                value.sz.unwrap_or(1.0),
            ],
        )
    }
}

#[derive(Debug, Clone)]
pub struct AnimKeyframe {
    transforms: Vec<PartialTransform>,
}

impl AnimKeyframe {
    pub fn new(
        descriptor: &AnimDescriptor,

        bytes: &[u8],
        // precision_specifiers: &PrecisionSpecifiers,
        // pack_formats: &[PackFormat],
        // shorts: &[i16],
    ) -> Result<Self, AssetParseError> {
        let mut transforms = vec![];

        if bytes.len() < (descriptor.bits_per_keyframe_exact() + 4) / 8 {
            return Err(AssetParseError::ErrorParsingDescriptor);
        }

        if descriptor.num_bones as usize != descriptor.pack_formats.len() {
            return Err(AssetParseError::InvalidDataViews(
                "Number of transforms per keyframe does not match the number of pack formats."
                    .to_string(),
            ));
        }

        let mut stream = BitStream::new(bytes);

        // Map each bit per channel into delta offsets
        let transform_deltas = descriptor
            .bits_per_channel
            .iter()
            .map(|num_bits| {
                stream
                    .read(*num_bits as usize)
                    .map(|v| v as i32)
                    .map_err(|e| {
                        AssetParseError::InvalidDataViews(format!(
                            "Unable to read {num_bits} bits from keyframe of length {}: {e}",
                            bytes.len(),
                        ))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut zipped = descriptor.shorts.iter().zip(transform_deltas);

        // Reconstruct the transforms using the delta offsets
        for i in 0..descriptor.num_bones as usize {
            let mut transform = PartialTransform::default();

            let format = unsafe { descriptor.pack_formats.get_unchecked(i) };

            let mut get_transform_val = |scale: f32| -> Result<f32, AssetParseError> {
                let Some((short, delta)) = zipped.next() else {
                    // TODO: Figure out why these 2 are different
                    return Ok(0.0);
                    // return Err(AssetParseError::InvalidDataViews(format!(
                    //     "Not enough transform values or shorts available in keyframe ({} shorts available, {} channels have bit counts)",
                    //     descriptor.shorts.len(),
                    //     descriptor.bits_per_channel.len(),
                    // )));
                };

                Ok((i32::from(*short) + delta) as f32 * scale)
            };

            if format.qx != AnimValueUsageType::Unused {
                transform.qx = Some(get_transform_val(
                    descriptor.precision_specifiers.quat_constant(),
                )?);
            }
            if format.qy != AnimValueUsageType::Unused {
                transform.qy = Some(get_transform_val(
                    descriptor.precision_specifiers.quat_constant(),
                )?);
            }
            if format.qz != AnimValueUsageType::Unused {
                transform.qz = Some(get_transform_val(
                    descriptor.precision_specifiers.quat_constant(),
                )?);
            }

            if let Some(translation) = &format.translation {
                if translation.x != AnimValueUsageType::Unused {
                    transform.tx = Some(get_transform_val(
                        descriptor.precision_specifiers.pos_constant(),
                    )?);
                }
                if translation.y != AnimValueUsageType::Unused {
                    transform.ty = Some(get_transform_val(
                        descriptor.precision_specifiers.pos_constant(),
                    )?);
                }
                if translation.z != AnimValueUsageType::Unused {
                    transform.tz = Some(get_transform_val(
                        descriptor.precision_specifiers.pos_constant(),
                    )?);
                }
            }

            if let Some(scale) = &format.scale {
                if scale.x != AnimValueUsageType::Unused {
                    transform.sx = Some(get_transform_val(
                        descriptor.precision_specifiers.scale_constant(),
                    )?);
                }
                if scale.y != AnimValueUsageType::Unused {
                    transform.sy = Some(get_transform_val(
                        descriptor.precision_specifiers.scale_constant(),
                    )?);
                }
                if scale.z != AnimValueUsageType::Unused {
                    transform.sz = Some(get_transform_val(
                        descriptor.precision_specifiers.scale_constant(),
                    )?);
                }
            }

            transforms.push(transform);
        }

        Ok(Self { transforms })
    }

    pub fn transforms(&self) -> &[PartialTransform] {
        &self.transforms
    }

    pub fn as_node_transforms(&self) -> Vec<NodeTransform> {
        self.transforms()
            .iter()
            .map(|t| NodeTransform::from(t.clone()))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub enum AnimError {
    SizeMismatch,
    InvalidInput,
    UnsupportedOutputType,
}

#[derive(Debug, Clone)]
pub struct Anim {
    descriptor: AnimDescriptor,
    keyframes: Vec<AnimKeyframe>,
}

#[derive(Debug, Clone, Default)]
pub struct BoneAnimChannel {
    pub translation: Option<Vec<[f32; 3]>>,
    pub rotation: Option<Vec<[f32; 4]>>,
    pub scale: Option<Vec<[f32; 3]>>,
}

impl Anim {
    pub fn new(descriptor: AnimDescriptor) -> Self {
        Anim {
            descriptor,
            keyframes: Default::default(),
        }
    }

    pub fn descriptor(&self) -> &AnimDescriptor {
        &self.descriptor
    }

    pub fn keyframes(&self) -> &[AnimKeyframe] {
        &self.keyframes
    }

    // pub fn get_channels(&self) -> Vec<Vec<NodeTransform>> {
    //     let num_channels = self
    //         .keyframes
    //         .iter()
    //         .fold(0usize, |acc, kf| acc.max(kf.transforms.len()));
    //
    //     let mut channels = vec![vec![]; num_channels];
    //
    //     for keyframe in &self.keyframes {
    //         for (i, transform) in keyframe.as_node_transforms().into_iter().enumerate() {
    //             channels[i].push(transform);
    //         }
    //     }
    //
    //     channels
    // }

    pub fn get_bone_anim_channels(&self) -> Vec<BoneAnimChannel> {
        let num_channels = self
            .keyframes
            .iter()
            .fold(0usize, |acc, kf| acc.max(kf.transforms.len()));

        let mut bone_anim_channels = vec![BoneAnimChannel::default(); num_channels];

        if let Some(keyframe) = self.keyframes.first() {
            keyframe
                .transforms
                .iter()
                .take(num_channels)
                .enumerate()
                .for_each(|(i, transform)| {
                    if transform.tx.is_some() || transform.ty.is_some() || transform.tz.is_some() {
                        bone_anim_channels[i].translation = Some(vec![]);
                    }

                    if transform.qx.is_some() || transform.qy.is_some() || transform.qz.is_some() {
                        bone_anim_channels[i].rotation = Some(vec![]);
                    }

                    if transform.sx.is_some() || transform.sy.is_some() || transform.sz.is_some() {
                        bone_anim_channels[i].scale = Some(vec![]);
                    }
                })
        }

        let mut prev_quat: Option<Quaternion> = None;

        for keyframe in &self.keyframes {
            keyframe
                .transforms
                .iter()
                .take(num_channels)
                .enumerate()
                .for_each(|(i, transform)| {
                    if transform.tx.is_some() || transform.ty.is_some() || transform.tz.is_some() {
                        bone_anim_channels[i].translation.as_mut().unwrap().push([
                            transform.tx.unwrap_or(0.0),
                            transform.ty.unwrap_or(0.0),
                            transform.tz.unwrap_or(0.0),
                        ]);
                    }

                    if transform.qx.is_some() || transform.qy.is_some() || transform.qz.is_some() {
                        bone_anim_channels[i].rotation.as_mut().unwrap().push({
                            let (x, y, z) = (
                                transform.qx.unwrap_or(0.0),
                                transform.qy.unwrap_or(0.0),
                                transform.qz.unwrap_or(0.0),
                            );

                            let w = (1.0 - (x.powf(2.0) + y.powf(2.0) + z.powf(2.0)))
                                .max(0.0)
                                .sqrt();

                            let mut q = Quaternion { x, y, z, w };

                            if let Some(q2) = prev_quat.take() {
                                if q.dot(&q2) < 0.0 {
                                    q = -q;
                                }
                            }

                            prev_quat = Some(q.clone());

                            q.to_array()
                        });
                    }

                    if transform.sx.is_some() || transform.sy.is_some() || transform.sz.is_some() {
                        bone_anim_channels[i].scale.as_mut().unwrap().push([
                            transform.sx.unwrap_or(1.0),
                            transform.sy.unwrap_or(1.0),
                            transform.sz.unwrap_or(1.0),
                        ]);
                    }
                })
        }

        bone_anim_channels
    }
}

impl AssetDescriptor for AnimDescriptor {
    fn from_bytes(data: &[u8]) -> Result<Self, AssetParseError> {
        let mut cur = Cursor::new(data);

        let mut magic = [0u8; 4];

        cur.read_exact(&mut magic)?;

        // Stored backwards in asset
        if &magic != b"MINA" {
            return Err(AssetParseError::ErrorParsingDescriptor);
        }

        let inverse_divisor = cur.read_f32::<LittleEndian>()?;
        let duration = cur.read_f32::<LittleEndian>()?;
        let c_vals_ptr: u32 = cur.read_u32::<LittleEndian>()?;
        let some_ptr_1: u32 = cur.read_u32::<LittleEndian>()?;

        let transforms_per_keyframe = cur.read_u16::<LittleEndian>()?;
        let unused_1 = cur.read_u16::<LittleEndian>()?;
        let num_keyframes = cur.read_u16::<LittleEndian>()?;
        let unused_2 = cur.read_u16::<LittleEndian>()?;

        let precision_specifiers = cur.read_u32::<LittleEndian>()?.into();

        let some_ptr_2 = cur.read_u32::<LittleEndian>()?;
        let some_u32_1 = cur.read_u32::<LittleEndian>()?;
        let tail_data_ptr = cur.read_u32::<LittleEndian>()?;
        let some_u32_2 = cur.read_u32::<LittleEndian>()?;

        let some_u32_3 = cur.read_u32::<LittleEndian>()?;
        let some_u32_4 = cur.read_u32::<LittleEndian>()?;
        let some_u32_5 = cur.read_u32::<LittleEndian>()?;
        let some_u32_6 = cur.read_u32::<LittleEndian>()?;

        let header_size = cur.read_u16::<LittleEndian>()?;
        let section1_size = cur.read_u16::<LittleEndian>()?;
        let section2_size = cur.read_u16::<LittleEndian>()?;

        let keyframe_size: u16 = cur.read_u16::<LittleEndian>()?;

        let some_float: f32 = cur.read_f32::<LittleEndian>()?;

        let pack_formats = (0..transforms_per_keyframe)
            .map(|_| PackFormat::from_mut_cursor(&mut cur))
            .collect::<Result<Vec<_>, _>>()?;

        let shorts = (0..(section1_size / 2))
            .map(|_| cur.read_i16::<LittleEndian>())
            .collect::<Result<Vec<_>, _>>()?;

        let bits_per_channel = (0..(section2_size))
            .map(|_| cur.read_u8())
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flat_map(|v| [(v & 0b1111) + 1, ((v >> 4) & 0b1111) + 1])
            .collect::<Vec<u8>>();

        let mut keyframe_bytes = vec![];

        cur.read_to_end(&mut keyframe_bytes)?;

        Ok(AnimDescriptor {
            magic,
            inverse_divisor,
            duration,
            c_vals_ptr,
            some_ptr_1,
            num_bones: transforms_per_keyframe,
            unused_1,
            num_keyframes,
            unused_2,
            precision_specifiers,
            some_ptr_2,
            some_u32_1,
            tail_data_ptr,
            some_u32_2,
            some_u32_3,
            some_u32_4,
            some_u32_5,
            some_u32_6,
            header_size,
            section1_size,
            section2_size,
            keyframe_size,
            some_float,
            pack_formats,
            shorts,
            bits_per_channel,
            keyframe_bytes,
        })
    }

    fn size(&self) -> usize {
        todo!();
    }

    fn asset_type() -> AssetType {
        AssetType::ResAnim
    }

    fn to_bytes(&self) -> Result<Vec<u8>, AssetParseError> {
        todo!();
    }
}

impl AssetLike for Anim {
    type Descriptor = AnimDescriptor;

    fn new(descriptor: &Self::Descriptor, _: &VirtualResource) -> Result<Self, AssetParseError> {
        if descriptor.keyframe_bytes.is_empty()
            || descriptor.keyframe_size == 0
            || descriptor.keyframe_size as usize > descriptor.keyframe_bytes.len()
        {
            return Err(AssetParseError::ErrorParsingDescriptor);
        }

        let keyframes = descriptor
            .keyframe_bytes
            .chunks_exact(descriptor.keyframe_size as usize)
            .map(|chunk| AnimKeyframe::new(descriptor, chunk))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            descriptor: descriptor.clone(),
            keyframes,
        })
    }

    fn get_descriptor(&self) -> Self::Descriptor {
        self.descriptor.clone()
    }

    fn get_resource_chunks(&self) -> Option<Vec<Vec<u8>>> {
        None
    }
}
