use crate::asset::AssetData;

#[derive(Debug, Clone)]
pub struct CueList {
    groups: Vec<CueGroup>,
}

#[derive(Debug, Clone)]
pub struct CueGroup {
    pub name: String,
    pub cues: Vec<String>,
}

impl CueGroup {
    pub fn new(name: String, cues: Option<Vec<String>>) -> Self {
        Self {
            name,
            cues: cues.unwrap_or_default(),
        }
    }
}

impl CueList {
    /// Example
    /// if let Some((group, cue)) = cue_list.get_cue("")
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

    pub fn groups(&self) -> &[CueGroup] {
        &self.groups
    }
}

pub struct CueListIterator<'cl> {
    cue_list_descriptor: &'cl CueList,
    current_group_index: usize,
    current_cue_index: usize,
}

impl<'cl> CueListIterator<'cl> {
    pub(crate) fn new(descriptor: &'cl CueList) -> Self {
        Self {
            cue_list_descriptor: descriptor,
            current_group_index: 0,
            current_cue_index: usize::MAX,
        }
    }
}

impl<'cl> Iterator for CueListIterator<'cl> {
    type Item = (&'cl String, &'cl String);

    fn next(&mut self) -> Option<Self::Item> {
        self.current_cue_index = self.current_cue_index.wrapping_add(1);

        let group = match self
            .cue_list_descriptor
            .groups
            .get(self.current_group_index)
        {
            Some(v) => v,
            None => {
                return None;
            }
        };

        match group.cues.get(self.current_cue_index) {
            Some(next_cue) => {
                return Some((&group.name, next_cue));
            }
            None => {
                self.current_group_index += 1;
                self.current_cue_index = 0;

                if let Some(group) = self
                    .cue_list_descriptor
                    .groups
                    .get(self.current_group_index)
                    && let Some(cue) = group.cues.get(self.current_cue_index)
                {
                    return Some((&group.name, cue));
                }
            }
        }

        None
    }
}

impl CueList {
    pub fn cues(&self) -> CueListIterator<'_> {
        CueListIterator::new(self)
    }
}

impl TryFrom<crate::RawAssetData> for CueList {
    type Error = crate::Error;

    fn try_from(value: crate::RawAssetData) -> Result<Self, Self::Error> {
        let crate::RawAssetData {
            descriptor_bytes,
            resource_chunks: _,
        } = value;

        let s = String::from_utf8(descriptor_bytes)?;

        let lines: Vec<(String, String)> = s
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                let parts: Vec<&str> = line.split('\t').collect();

                // Must match format Ggroup\tname\n
                if parts.len() != 2 {
                    Err("parts len wrong".into())
                } else {
                    Ok((parts[0].to_string(), parts[1].to_string()))
                }
            })
            .collect::<Result<Vec<(String, String)>, crate::Error>>()?;

        let mut groups = vec![];

        let mut group = CueGroup {
            name: "".to_string(),
            cues: vec![],
        };

        for (group_name, entry) in lines {
            if group.name != group_name {
                if !group.cues.is_empty() {
                    groups.push(group);
                }

                group = CueGroup::new(group_name, None);
            }

            group.cues.push(entry)
        }

        Ok(Self { groups })
    }
}

impl TryFrom<CueList> for crate::RawAssetData {
    type Error = crate::Error;

    fn try_from(value: CueList) -> Result<Self, Self::Error> {
        // let mut bytes = Vec::new();

        let mut lines = vec![];

        if !value.validate() {
            return Err(crate::asset::AssetParseError::InvalidDataViews(
                "Failed to validate, empty string found.".to_string(),
            )
            .into());
        }

        for group in &value.groups {
            for cue in &group.cues {
                lines.push(format!("{}\t{}", group.name, cue));
            }
        }

        Ok(crate::RawAssetData {
            descriptor_bytes: lines.join("\n").chars().map(|c| c as u8).collect(),
            resource_chunks: vec![],
        })
    }
}

impl AssetData for CueList {
    const ASSET_TYPE: super::AssetType = super::AssetType::XCueList;
}

#[cfg(test)]
pub mod tests {
    use super::*;

    use ntest::timeout;

    #[test]
    #[timeout(1000)] // Make sure test runs in under 1 second
    fn cue_list_iterator() {
        let cue_counts = [3, 1, 2, 4];

        let mut cues = vec![];

        let groups: Vec<CueGroup> = cue_counts
            .iter()
            .enumerate()
            .map(|(i, cue_count)| {
                let group_num = i + 1;

                let group_name = format!("group{}", group_num);

                CueGroup::new(
                    group_name.clone(),
                    Some(
                        (0..*cue_count)
                            .map(|cue_i| {
                                let cue_name = format!("{}cue{}", group_name, cue_i + 1);
                                cues.push((group_name.clone(), cue_name.clone()));

                                cue_name
                            })
                            .collect(),
                    ),
                )
            })
            .collect();

        let cue_list = CueList { groups };

        assert_eq!(
            cue_list
                .cues()
                .map(|(s1, s2)| (s1.to_owned(), s2.to_owned()))
                .collect::<Vec<(String, String)>>(),
            cues
        )
    }
}
