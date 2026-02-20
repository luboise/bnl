mod push_buffer;
mod shader;
mod skeleton;
mod vertex_buffer;

pub use push_buffer::{DrawCall, NdBGPushBuffer, NdPushBuffer};
pub use shader::{NdShader2, NdShaderParam2};
pub use skeleton::NdSkeleton;
pub use vertex_buffer::*;

pub(crate) mod prelude {
    // External
    pub use gltf_writer::gltf::{self, GltfIndex};
    pub use serde::{Serialize, ser::SerializeMap};

    // Internal
    pub use super::ModelSlice;
    pub use crate::asset::AssetParseError;
    pub use crate::asset::model::gltf::NdGltfContext;
    pub use crate::asset::model::nd::{NdHeader, NdNode};

    pub use super::NdError;

    pub(crate) use crate::VirtualResource;
    pub(crate) use byteorder::{LittleEndian, ReadBytesExt};
    pub(crate) use std::io::{Cursor, Read, Seek, SeekFrom};
}

use std::{
    collections::HashMap,
    fmt::Display,
    io::{self},
    iter::{self},
};

use serde::{Serialize, ser::SerializeMap};

use crate::asset::{
    model::nd::{shader::NdShaderParam2Payload, skeleton::Bone},
    param::KnownUnknown,
};

use prelude::*;

#[derive(Debug)]
pub enum NdError {
    UnknownType,
    CreationFailure(String),
}

impl From<io::Error> for NdError {
    fn from(e: io::Error) -> Self {
        Self::CreationFailure(e.to_string())
    }
}

pub trait NdNode {
    fn children<'a>(&'a self) -> impl Iterator<Item = &'a Nd> + 'a {
        let child = self.header().first_child.as_deref();

        iter::successors(child, |node| node.header().next_sibling.as_deref())
    }

    fn header(&self) -> &NdHeader;

    fn add_gltf_node(
        &self,
        virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError>;

    fn insert_into_gltf_heirarchy(
        &self,
        virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        let node_index_opt = self.add_gltf_node(virtual_res, ctx)?;

        let type_string = self.header().nd_type.to_string();

        /*
        let mut parent = GltfIndex::MAX;
        let mut grandparent: Option<GltfIndex> = Some(GltfIndex::MAX);
        */

        let indentation = String::from_utf8(vec![b' '; 4 * ctx.node_stack.len()]).unwrap();

        // Push self, then handle child, then unpush self
        if let Some(node_index) = &node_index_opt {
            ctx.push_node(*node_index);

            println!(
                "{}Pushing {} {}, onto stack.",
                &indentation, type_string, node_index
            );
        }

        if let Some(child) = self.header().first_child() {
            child.insert_into_gltf_heirarchy(virtual_res, ctx)?;
        }

        if node_index_opt.is_some() {
            ctx.pop_node();

            println!(
                "{}Removing {} {} from stack.",
                indentation,
                type_string,
                node_index_opt.unwrap()
            );
        }

        if let Some(next_sibling) = self.header().next_sibling() {
            next_sibling.insert_into_gltf_heirarchy(virtual_res, ctx)?;
        }

        Ok(node_index_opt)
    }
}

impl From<KnownNdType> for String {
    fn from(value: KnownNdType) -> Self {
        match value {
            KnownNdType::VertexBuffer => "ndVertexBuffer",
            KnownNdType::PushBuffer => "ndPushBuffer",
            KnownNdType::BGPushBuffer => "ndBGPushBuffer",
            KnownNdType::Shader2 => "ndShader2",
            KnownNdType::VertexShader => "ndVertexShader",
            KnownNdType::ShaderParam2 => "ndShaderParam2",
            KnownNdType::Group => "ndGroup",
            KnownNdType::Skeleton => "ndSkeleton",
        }
        .to_string()
    }
}

pub(crate) const NDHEADER_SIZE: usize = 32;

#[derive(Debug, Clone)]
pub struct NdHeader {
    pub name_ptr: u32,
    pub nd_type: NdType,
    pub unknown_u16: u16, // Possibly index
    pub unknown_ptr1: u32,
    pub unknown_ptr2: u32,
    pub unknown_u32: u32,
    pub first_child_ptr: u32,
    pub next_sibling_ptr: u32,
    pub parent_ptr: u32,

    // DO NOT SERIALISE
    first_child: Option<Box<Nd>>,
    next_sibling: Option<Box<Nd>>,
}

impl Serialize for NdHeader {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(None)?;

        map.serialize_entry("type", &self.nd_type.to_string())?;

        let children: Vec<&Nd> = self.children().collect();
        map.serialize_entry("children", &children)?;

        map.end()
    }
}

impl NdHeader {
    pub fn from_bytes(
        ctx: &mut ModelReadContext,
        bytes: &[u8],
        header_start: u32,
    ) -> Result<NdHeader, NdError> {
        let mut cur = Cursor::new(bytes);

        cur.seek(SeekFrom::Start(header_start as u64))?;

        let name_ptr = cur.read_u32::<LittleEndian>()?;

        // TODO: Sanity check name against name ptr
        let type_u16 = cur.read_u16::<LittleEndian>()?;
        let unknown_u16 = cur.read_u16::<LittleEndian>()?;

        let unknown_ptr1 = cur.read_u32::<LittleEndian>()?;
        let unknown_ptr2 = cur.read_u32::<LittleEndian>()?;
        let unknown_u32 = cur.read_u32::<LittleEndian>()?;

        let first_child_ptr = cur.read_u32::<LittleEndian>()?;
        let next_sibling_ptr = cur.read_u32::<LittleEndian>()?;
        let parent_ptr = cur.read_u32::<LittleEndian>()?;

        // Processing

        let mut name_cur = cur.clone();
        name_cur.seek(SeekFrom::Start(name_ptr as u64))?;

        let mut chars = vec![];

        let mut c = name_cur.read_u8()?;

        while c != 0 {
            chars.push(c);
            c = name_cur.read_u8()?;
        }

        let name = String::from_utf8(chars).map_err(|e| {
            NdError::CreationFailure(format!("Failed to parse nd string name\n{}", e))
        })?;

        let nd_type: NdType = name.into();

        let first_child = match first_child_ptr {
            0 => None,
            _ => Some(
                Nd::new(
                    ctx,
                    ModelSlice {
                        slice: bytes,
                        read_start: first_child_ptr as usize,
                    },
                )?
                .into(),
            ),
        };

        let next_sibling = match next_sibling_ptr {
            0 => None,
            _ => Some(
                Nd::new(
                    ctx,
                    ModelSlice {
                        slice: bytes,
                        read_start: next_sibling_ptr as usize,
                    },
                )?
                .into(),
            ),
        };

        Ok(NdHeader {
            name_ptr,
            nd_type,
            unknown_u16,
            unknown_ptr1,
            unknown_ptr2,
            unknown_u32,
            first_child_ptr,
            next_sibling_ptr,
            parent_ptr,
            //
            first_child,
            next_sibling,
        })
    }

    pub fn children(&self) -> impl Iterator<Item = &Nd> {
        iter::successors(self.first_child(), |nd| nd.header().next_sibling())
    }

    pub fn first_child(&self) -> Option<&Nd> {
        self.first_child.as_deref()
    }

    pub fn next_sibling(&self) -> Option<&Nd> {
        self.next_sibling.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum KnownNdType {
    VertexBuffer,
    PushBuffer,
    BGPushBuffer,
    Shader2,
    VertexShader,
    ShaderParam2,
    Skeleton,
    Group,
}

impl Display for KnownNdType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KnownNdType::VertexBuffer => write!(f, "ndVertexBuffer"),
            KnownNdType::PushBuffer => write!(f, "ndPushBuffer"),
            KnownNdType::BGPushBuffer => write!(f, "ndBGPushBuffer"),
            KnownNdType::Shader2 => write!(f, "ndShader2"),
            KnownNdType::VertexShader => write!(f, "ndVertexShader"),
            KnownNdType::ShaderParam2 => write!(f, "ndShaderParam2"),
            KnownNdType::Skeleton => write!(f, "ndSkeleton"),
            KnownNdType::Group => write!(f, "ndGroup"),
        }
    }
}

impl TryFrom<String> for KnownNdType {
    type Error = NdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_ref() {
            "ndVertexBuffer" => Ok(KnownNdType::VertexBuffer),
            "ndPushBuffer" => Ok(KnownNdType::PushBuffer),
            "ndBGPushBuffer" => Ok(KnownNdType::BGPushBuffer),
            "ndShader2" => Ok(KnownNdType::Shader2),
            "ndVertexShader" => Ok(KnownNdType::VertexShader),
            "ndShaderParam2" => Ok(KnownNdType::ShaderParam2),
            "ndSkeleton" => Ok(KnownNdType::Skeleton),
            "ndGroup" => Ok(KnownNdType::Group),
            _ => Err(NdError::UnknownType),
        }
    }
}

type NdType = KnownUnknown<KnownNdType, String>;

impl ToString for NdType {
    fn to_string(&self) -> String {
        match self {
            KnownUnknown::Known(val) => val.to_string(),
            KnownUnknown::Unknown(val) => val.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NdUnknown {
    header: NdHeader,
}

impl NdNode for NdUnknown {
    fn header(&self) -> &NdHeader {
        &self.header
    }

    fn add_gltf_node(
        &self,
        _virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        Ok(None)
    }
}

impl NdUnknown {
    pub(crate) fn header(&self) -> &NdHeader {
        &self.header
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum Nd {
    Skeleton(NdSkeleton),
    VertexBuffer(NdVertexBuffer),
    PushBuffer(NdPushBuffer),
    BGPushBuffer(NdBGPushBuffer),
    Group(NdGroup),
    Shader2(NdShader2),
    VertexShader(NdVertexShader),
    ShaderParam2(NdShaderParam2),
    Unknown(NdUnknown),
}

impl NdNode for Nd {
    fn header(&self) -> &NdHeader {
        match self {
            Nd::VertexBuffer(val) => val.header(),
            Nd::PushBuffer(val) => val.header(),
            Nd::BGPushBuffer(val) => val.header(),
            Nd::Group(val) => val.header(),
            Nd::Unknown(val) => val.header(),
            Nd::Shader2(val) => val.header(),
            Nd::VertexShader(val) => val.header(),
            Nd::ShaderParam2(val) => val.header(),
            Nd::Skeleton(val) => val.header(),
        }
    }

    fn add_gltf_node(
        &self,
        virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        match self {
            Nd::VertexBuffer(nd) => nd.add_gltf_node(virtual_res, ctx),
            Nd::PushBuffer(nd) => nd.add_gltf_node(virtual_res, ctx),
            Nd::BGPushBuffer(nd) => nd.add_gltf_node(virtual_res, ctx),
            Nd::Group(nd) => nd.add_gltf_node(virtual_res, ctx),
            Nd::Shader2(nd) => nd.add_gltf_node(virtual_res, ctx),
            Nd::VertexShader(nd) => nd.add_gltf_node(virtual_res, ctx),
            Nd::ShaderParam2(nd) => nd.add_gltf_node(virtual_res, ctx),
            Nd::Skeleton(nd) => nd.add_gltf_node(virtual_res, ctx),
            Nd::Unknown(nd) => nd.add_gltf_node(virtual_res, ctx),
        }
    }
}

impl<'a> Iterator for NdIterator<'a> {
    type Item = &'a Nd;

    fn next(&mut self) -> Option<Self::Item> {
        self.current_nd?;

        // If sibling
        if let Some(x) = self.current_nd.unwrap().header().next_sibling.as_deref() {
            self.current_nd = Some(x);
            return Some(x);
        }

        // If child
        if let Some(child) = self.base_nd.header().first_child() {
            self.current_nd = Some(child);
            self.base_nd = child;

            return Some(child);
        }

        unreachable!()
    }
}

impl Nd {
    pub fn heirarchy(&self) -> impl Iterator<Item = &Nd> {
        NdIterator::new(self)
    }
}

struct NdIterator<'a> {
    base_nd: &'a Nd,
    current_nd: Option<&'a Nd>,
}

impl<'a> NdIterator<'a> {
    pub fn new(nd: &'a Nd) -> Self {
        Self {
            base_nd: nd,
            current_nd: Some(nd),
        }
    }
}

/*
impl Serialize for Nd {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.header().serialize(serializer)
    }
}
*/

pub struct ModelReadContext<'a> {
    key_value_map: &'a HashMap<String, Vec<u8>>,
}

impl<'a> ModelReadContext<'a> {
    pub fn new(key_value_map: &'a HashMap<String, Vec<u8>>) -> Self {
        Self { key_value_map }
    }

    pub fn get_bone_name(&self, bone_index: u32) -> Option<&str> {
        self.key_value_map.iter().find_map(|(k, v)| {
            (is_bone_name(k)
                && v.len() == 4
                && u32::from_le_bytes(v.as_slice().try_into().unwrap()) == bone_index)
                .then_some(k.as_str())
        })
    }
}

pub fn is_bone_name<S: AsRef<str>>(s: S) -> bool {
    ["BASE", "MID", "joint3"].contains(&s.as_ref())
}

pub struct ModelSlice<'a> {
    pub(crate) slice: &'a [u8],
    pub(crate) read_start: usize,
}

impl<'a> ModelSlice<'a> {
    pub fn slice(&self) -> &'a [u8] {
        self.slice
    }

    pub fn nd_start(&self) -> usize {
        self.read_start
    }

    pub fn at(&self, read_start: usize) -> Self {
        ModelSlice {
            slice: self.slice,
            read_start,
        }
    }

    pub fn new_cursor(&self) -> Cursor<&[u8]> {
        let mut cur = Cursor::new(self.slice);
        cur.seek(SeekFrom::Start(self.read_start as u64)).unwrap();

        cur
    }
}

impl Nd {
    pub fn new(ctx: &mut ModelReadContext, model_slice: ModelSlice) -> Result<Nd, NdError> {
        let slice = model_slice.slice();

        let mut cur = Cursor::new(slice);

        let header = NdHeader::from_bytes(ctx, slice, model_slice.read_start as u32)?;

        cur.seek(SeekFrom::Start(32 + model_slice.read_start as u64))?;

        if let KnownUnknown::Known(nd_type) = &header.nd_type.clone() {
            match nd_type {
                KnownNdType::VertexBuffer => {
                    let resource_views_ptr = cur.read_u32::<LittleEndian>()?;
                    let num_resource_views = cur.read_u32::<LittleEndian>()?;

                    let mut resource_views = Vec::with_capacity(num_resource_views as usize);

                    for _ in 0..num_resource_views {
                        resource_views
                            .push(res_view::VertexBufferResourceView::from_cursor(&mut cur)?);
                    }

                    Ok(Nd::VertexBuffer(NdVertexBuffer {
                        header,
                        resource_views_ptr,
                        num_resource_views,
                        resource_views,
                    }))
                }
                KnownNdType::PushBuffer | KnownNdType::BGPushBuffer => {
                    let num_draws = cur.read_u32::<LittleEndian>()?;
                    let unknown_u32_1 = cur.read_u32::<LittleEndian>()?;
                    let unknown_u32_2 = cur.read_u32::<LittleEndian>()?;
                    let unknown_u32_3 = cur.read_u32::<LittleEndian>()?;

                    let data_pointers_start = cur.read_u32::<LittleEndian>()?;
                    let primitive_types_list_ptr = cur.read_u32::<LittleEndian>()?;
                    let vertex_counts_list_ptr = cur.read_u32::<LittleEndian>()?;

                    let prevent_culling_flag = cur.read_u8()?;
                    let mut padding = [0u8; 3];

                    cur.read_exact(&mut padding)?;

                    let mut data_ptr_cur = cur.clone();
                    data_ptr_cur.seek(SeekFrom::Start(data_pointers_start as u64))?;

                    let mut prim_type_ptr = cur.clone();
                    prim_type_ptr.seek(SeekFrom::Start(primitive_types_list_ptr as u64))?;

                    let mut vertex_counts_ptr = cur.clone();
                    vertex_counts_ptr.seek(SeekFrom::Start(vertex_counts_list_ptr as u64))?;

                    let mut draw_calls = Vec::with_capacity(num_draws as usize);

                    // TODO: FIGURE OUT IF THIS GOES HERE
                    let mut min = u32::MAX;
                    let mut max = u32::MIN;

                    for _ in 0..num_draws as usize {
                        let data_ptr = data_ptr_cur.read_u32::<LittleEndian>()?;
                        let prim_type = prim_type_ptr.read_u32::<LittleEndian>()?.into();
                        let num_vertices = vertex_counts_ptr.read_u32::<LittleEndian>()?;
                        let data_size = num_vertices * size_of::<u16>() as u32;

                        if data_ptr < min {
                            min = data_ptr;
                        }
                        if data_ptr + data_size > max {
                            max = data_ptr + data_size;
                        }

                        draw_calls.push(DrawCall {
                            data_ptr,
                            prim_type,
                            num_vertices,
                        });
                    }

                    let push_buffer_base = min;
                    let push_buffer_size = (max - min) as usize;

                    let buffer_bytes = slice
                        [push_buffer_base as usize..push_buffer_base as usize + push_buffer_size]
                        .to_vec();

                    let push_buffer = NdPushBuffer {
                        header,
                        num_draws,
                        unknown_u32_1,
                        unknown_u32_2,
                        unknown_u32_3,
                        //
                        data_pointers_start,
                        primitive_types_list_ptr,
                        vertex_counts_list_ptr,
                        //
                        prevent_culling_flag,
                        padding,
                        //
                        buffer_bytes,
                        push_buffer_base,
                        push_buffer_size: push_buffer_size as u32,

                        draw_calls,
                    };

                    if *nd_type == KnownNdType::BGPushBuffer {
                        let unknown_ptr_1 = cur.read_u32::<LittleEndian>()?;
                        let unknown_ptr_2 = cur.read_u32::<LittleEndian>()?;

                        Ok(Nd::BGPushBuffer(NdBGPushBuffer {
                            push_buffer,
                            unknown_ptr_1,
                            unknown_ptr_2,
                        }))
                    } else {
                        Ok(Nd::PushBuffer(push_buffer))
                    }
                }
                KnownNdType::Group => {
                    // NdGroup spotted
                    Ok(Nd::Group(NdGroup { header }))
                }
                KnownNdType::ShaderParam2 => {
                    let main_payload_ptr = cur.read_u32::<LittleEndian>()?;
                    let sub_payload_ptr = cur.read_u32::<LittleEndian>()?;

                    let main_payload = NdShaderParam2Payload::from_model_slice(
                        &model_slice.at(main_payload_ptr as usize),
                    )?;

                    let sub_payload = match sub_payload_ptr {
                        0 => None,
                        val => Some(NdShaderParam2Payload::from_model_slice(
                            &model_slice.at(val as usize),
                        )?),
                    };

                    Ok(Nd::ShaderParam2(NdShaderParam2 {
                        header,
                        main_payload,
                        sub_payload,
                    }))
                }
                KnownNdType::Skeleton => {
                    let num_bones = cur.read_u32::<LittleEndian>()?;
                    let bones_ptr = cur.read_u32::<LittleEndian>()?;

                    let bones = if bones_ptr != 0 && num_bones > 0 {
                        let mut bones = Vec::with_capacity(num_bones as usize);

                        cur.seek(SeekFrom::Start(bones_ptr as u64))?;

                        for i in 0..(num_bones as u32) {
                            bones.push(Bone {
                                name: ctx.get_bone_name(i).map(|v| v.into()),
                                parent_id: cur.read_u16::<LittleEndian>()?,
                                id: cur.read_u16::<LittleEndian>()?,
                                local_transform: [
                                    cur.read_f32::<LittleEndian>()?,
                                    cur.read_f32::<LittleEndian>()?,
                                    cur.read_f32::<LittleEndian>()?,
                                ],
                                global_transform: [
                                    cur.read_f32::<LittleEndian>()?,
                                    cur.read_f32::<LittleEndian>()?,
                                    cur.read_f32::<LittleEndian>()?,
                                ],
                                sentinel: cur.read_u32::<LittleEndian>()?.to_le_bytes(),
                            });
                        }

                        bones
                    } else {
                        vec![]
                    };

                    Ok(Nd::Skeleton(NdSkeleton { header, bones }))
                }
                KnownNdType::Shader2 => Ok(Nd::Shader2(NdShader2 { header })),
                KnownNdType::VertexShader => Ok(Nd::VertexShader(NdVertexShader { header })),
            }
        } else {
            Ok(Nd::Unknown(NdUnknown { header }))
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NdGroup {
    header: NdHeader,
}

impl NdNode for NdGroup {
    fn header(&self) -> &NdHeader {
        &self.header
    }

    fn add_gltf_node(
        &self,
        _virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        Ok(Some(
            ctx.gltf
                .add_node(gltf::Node::new(Some("ndGroup".to_string()))),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn get_test_bytes() -> Vec<u8> {
        let test_path = std::path::Path::new(file!())
            .parent()
            .expect("Unable to get parent directory of test.")
            .join("test_meshes")
            .join("test_mesh_0");

        fs::read(test_path).expect("Unable to read test input.")
    }

    fn get_test_file(filename: &str) -> Vec<u8> {
        let test_path = std::path::Path::new(file!())
            .parent()
            .expect("Unable to get parent directory of test.")
            .join("test_meshes")
            .join(filename);

        fs::read(&test_path).expect("Unable to get test file")
    }

    #[test]
    fn nd_header() {
        let bytes = get_test_bytes();
        NdHeader::from_bytes(
            &mut ModelReadContext::new(&Default::default()),
            &bytes,
            0x34,
        )
        .expect("Unable to create NdHeader");
    }

    #[test]
    fn nd_parse_test() {
        let bytes = get_test_bytes();

        Nd::new(
            &mut ModelReadContext::new(&Default::default()),
            ModelSlice {
                slice: &bytes,
                read_start: 0x34,
            },
        )
        .expect("Unable to create ND");
    }

    #[test]
    fn nd_shader_param2() {
        let bytes = get_test_file("test_ndShaderParam2_1");

        let nd = Nd::new(
            &mut ModelReadContext::new(&Default::default()),
            ModelSlice {
                slice: &bytes,
                read_start: 0,
            },
        )
        .expect("Unable to create ND");

        if let Nd::ShaderParam2(sp2) = nd {
            let attribute_map = &sp2.main_payload().attribute_map();

            assert_eq!(attribute_map.len(), 2, "Attribute map is wrong size.");

            assert_eq!(
                sp2.num_bound_textures(),
                2,
                "Number of bound textures is wrong."
            );

            assert_eq!(attribute_map.len(), 2, "Attribute map is wrong size.");
        } else {
            panic!(
                "nd has wrong type {:?}, expected ndShaderParam2.",
                dbg!(&nd)
            );
        }
    }
}
