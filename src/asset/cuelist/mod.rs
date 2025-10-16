use crate::{
    VirtualResource,
    asset::{AssetDescriptor, AssetLike, AssetParseError, AssetType},
};

#[derive(Debug, Clone)]
pub struct CueList {
    descriptor: CueListDescriptor,
    data: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct CueGroup {
    name: String,
    cues: Vec<String>,
}

impl CueGroup {
    pub fn new(name: String, cues: Option<Vec<String>>) -> Self {
        Self {
            name,
            cues: cues.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CueListDescriptor {
    groups: Vec<CueGroup>,
}

/// Example
/// if let Some((group, cue)) = cue_list.get_cue("")
impl CueListDescriptor {
    pub fn get_cue<S: Into<String>>(&self, cue: S) -> Option<(String, String)> {
        let s = cue.into();

        self.groups
            .iter()
            .find(|group| group.cues.contains(&s))
            .map(|group| (group.name.clone(), s))
    }

    pub fn validate(&self) -> bool {
        self.groups
            .iter()
            .all(|group| !group.name.is_empty() && group.cues.iter().all(|cue| !cue.is_empty()))
    }
}

impl AssetDescriptor for CueListDescriptor {
    fn from_bytes(data: &[u8]) -> Result<Self, AssetParseError> {
        let s = String::from_utf8(data.to_owned())
            .map_err(|_| AssetParseError::ErrorParsingDescriptor)?;

        let lines: Vec<(String, String)> = s
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| -> Result<(String, String), AssetParseError> {
                let parts: Vec<&str> = line.split('\t').collect();

                // Must match format Ggroup\tname\n
                if parts.len() != 2 {
                    return Err(AssetParseError::ErrorParsingDescriptor);
                }

                Ok((parts[0].to_string(), parts[1].to_string()))
            })
            .collect::<Result<Vec<(String, String)>, AssetParseError>>()?;

        let mut descriptor = CueListDescriptor { groups: vec![] };

        let mut group = CueGroup {
            name: "".to_string(),
            cues: vec![],
        };

        for (group_name, entry) in lines {
            if group.name != group_name {
                if !group.cues.is_empty() {
                    descriptor.groups.push(group);
                }

                group = CueGroup::new(group_name, None);
            }

            group.cues.push(entry)
        }

        Ok(descriptor)
    }

    fn to_bytes(&self) -> Result<Vec<u8>, AssetParseError> {
        // let mut bytes = Vec::new();

        let mut lines = vec![];

        if !self.validate() {
            return Err(AssetParseError::InvalidDataViews(
                "Failed to validate, empty string found.".to_string(),
            ));
        }

        for group in &self.groups {
            for cue in &group.cues {
                lines.push(format!("{}\t{}", group.name, cue));
            }
        }

        Ok(lines.join("\n").chars().map(|c| c as u8).collect())
    }

    fn asset_type() -> AssetType {
        AssetType::ResXCueList
    }

    fn size(&self) -> usize {
        self.to_bytes().iter().len()
    }
}

impl AssetLike for CueList {
    type Descriptor = CueListDescriptor;

    fn new(
        descriptor: &Self::Descriptor,
        virtual_res: &VirtualResource,
    ) -> Result<Self, AssetParseError> {
        Ok(CueList {
            descriptor: descriptor.clone(),
            data: virtual_res
                .slices
                .iter()
                .map(|slice| slice.to_vec())
                .collect(),
        })
    }

    fn get_descriptor(&self) -> Self::Descriptor {
        self.descriptor.clone()
    }

    fn get_resource_chunks(&self) -> Option<Vec<Vec<u8>>> {
        match self.data.len() {
            0 => None,
            _ => Some(self.data.clone()),
        }
    }
}
