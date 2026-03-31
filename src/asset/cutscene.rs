use std::io::Read;

use byteorder::{LittleEndian, ReadBytesExt};

#[derive(Debug, Clone)]
pub struct Cutscene {
    pub descriptor: CutsceneDescriptor,
}

#[derive(Debug, Clone)]
pub struct CutsceneDescriptor {
    pub count_1: u8,
    pub count_2: u8,
    pub num_cameras: u8,
    pub num_animations: u8,
    pub length: f32,
    pub rest_raw: Vec<u8>,
}

impl super::AssetDescriptor for CutsceneDescriptor {
    fn from_bytes(data: &[u8]) -> Result<Self, super::AssetParseError> {
        let mut cur = std::io::Cursor::new(&data);
        let count_1 = cur.read_u8()?;
        let count_2 = cur.read_u8()?;
        let num_cameras = cur.read_u8()?;
        let num_animations = cur.read_u8()?;

        let length = cur.read_f32::<LittleEndian>()?;

        let mut raw = vec![];

        cur.read_to_end(&mut raw)?;

        Ok(CutsceneDescriptor {
            count_1,
            count_2,
            num_cameras,
            num_animations,
            length,
            rest_raw: raw,
        })
    }

    fn to_bytes(&self) -> Result<Vec<u8>, super::AssetParseError> {
        let mut ret = vec![
            self.count_1,
            self.count_2,
            self.num_cameras,
            self.num_animations,
        ];

        ret.extend(self.length.to_le_bytes());
        ret.extend(&self.rest_raw);

        Ok(ret)
    }

    fn size(&self) -> usize {
        8 + self.rest_raw.len()
    }

    fn asset_type() -> super::AssetType {
        super::AssetType::ResCutscene
    }
}

impl super::AssetLike for Cutscene {
    type Descriptor = CutsceneDescriptor;

    fn new(
        descriptor: &Self::Descriptor,
        _virtual_res: &crate::VirtualResource,
    ) -> Result<Self, super::AssetParseError> {
        let descriptor = descriptor.clone();
        Ok(Self { descriptor })
    }

    fn get_descriptor(&self) -> Self::Descriptor {
        self.descriptor.clone()
    }

    fn get_resource_chunks(&self) -> Option<Vec<Vec<u8>>> {
        None
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CutsceneMod {
    pub length: Option<f32>,
}

impl crate::modding::ModLike for CutsceneMod {
    type Descriptor = CutsceneDescriptor;

    fn apply(&self, descriptor: &mut Self::Descriptor) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(length) = self.length {
            descriptor.length = length;
        }

        Ok(())
    }
}
