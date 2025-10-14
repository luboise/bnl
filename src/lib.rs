pub(crate) mod d3d;

pub(crate) mod images;

pub mod asset;

mod bnl;
pub use bnl::*; // Want to make it just bnl::*, rather than bnl::bnl::*

use std::{cmp, fmt::Display};

use crate::asset::DataViewList;

pub mod game;

#[derive(Debug)]
pub(crate) struct VirtualResource<'a> {
    slices: Vec<&'a [u8]>,
}

#[derive(Debug)]
pub enum VirtualResourceError {
    OffsetOutOfBounds,
    SizeOutOfBounds,
}

impl Display for VirtualResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl VirtualResource<'_> {
    pub(crate) fn from_dvl<'a>(
        dataview_list: &DataViewList,
        bytes: &'a [u8],
    ) -> Result<VirtualResource<'a>, VirtualResourceError> {
        let views = dataview_list.views();

        let mut slices = Vec::new();

        for view in views {
            let offset = view.offset as usize;
            let size = view.size as usize;

            if offset > bytes.len() {
                return Err(VirtualResourceError::OffsetOutOfBounds);
            } else if bytes.len() - offset < size {
                return Err(VirtualResourceError::SizeOutOfBounds);
            }

            slices.push(&bytes[offset..offset + size]);
        }

        Ok(VirtualResource { slices })
    }

    pub fn get_bytes(
        &self,
        start_offset: usize,
        get_size: usize,
    ) -> Result<Vec<u8>, VirtualResourceError>
where {
        let end = self.len();

        if end < start_offset {
            return Err(VirtualResourceError::OffsetOutOfBounds);
        } else if end - start_offset < get_size {
            return Err(VirtualResourceError::SizeOutOfBounds);
        }

        let mut v = vec![0; get_size];

        let mut slice_start = 0usize;
        let mut total_written = 0usize;

        for slice in &self.slices {
            let slice_size = slice.len();

            // If this slice is part of the copy in any way
            if (slice_start + slice_size) > start_offset {
                let desired_cp_size = get_size - total_written;

                // Get start index
                let cp_i = start_offset.saturating_sub(slice_start);
                let cp_size = cmp::min(desired_cp_size, slice_size - cp_i);

                let cp_j = cp_i + cp_size;

                v[total_written..total_written + cp_size].copy_from_slice(&slice[cp_i..cp_j]);

                total_written += cp_size;

                if total_written > get_size {
                    return Err(VirtualResourceError::SizeOutOfBounds);
                } else if total_written == get_size {
                    break;
                }
            }

            slice_start += slice_size;
        }

        if total_written != get_size {
            return Err(VirtualResourceError::SizeOutOfBounds);
        }

        Ok(v)
    }

    pub fn get_all_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0x00; self.len()];

        let mut curr = 0usize;
        for slice in &self.slices {
            let copy_size = slice.len();

            bytes[curr..curr + copy_size].copy_from_slice(slice);

            curr += copy_size;
        }

        bytes
    }

    pub(crate) fn from_slices<'a>(slices: &'a [&[u8]]) -> VirtualResource<'a> {
        VirtualResource {
            slices: slices.to_vec(),
        }
    }

    pub fn len(&self) -> usize {
        self.slices
            .iter()
            .fold(0, |acc, slice: &&[u8]| -> usize { acc + (*slice).len() })
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn slices(&self) -> &[&[u8]] {
        &self.slices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn make_data<const N: usize>() -> [u8; N] {
        let mut arr = [0u8; N];
        let mut i = 0;
        while i < N {
            arr[i] = i as u8;
            i += 1;
        }

        arr
    }

    const DATA: [u8; 1000] = make_data::<1000>();

    #[test]
    fn read_across_slices() {
        let slices = [
            &DATA[0..100],
            &DATA[200..300],
            &DATA[400..500],
            &DATA[600..700],
        ];

        let virtual_res = VirtualResource::from_slices(&slices);

        let bytes = virtual_res.get_bytes(180, 200).unwrap();

        assert_eq!(bytes[0..20], DATA[280..300]);
        assert_eq!(bytes[20..120], DATA[400..500]);
        assert_eq!(bytes[120..200], DATA[600..680]);
    }
}
