use std::{
    error::Error,
    fs,
    io::{Cursor, Read, Seek, SeekFrom},
    path::PathBuf,
};

use byteorder::{LittleEndian, ReadBytesExt};
use serde::Deserialize;

pub fn dump_xwavebank_bytes(path: PathBuf) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(path)?;

    let mut cur = Cursor::new(&bytes);

    let mut wbnd_string = [0u8; 4];
    cur.read_exact(&mut wbnd_string)?;

    let header = XWavebankHeader {
        wbnd_string,
        unknown_count_1: cur.read_u32::<LittleEndian>()?,
        header_size: cur.read_u32::<LittleEndian>()?,
        wavebanks_ptr: cur.read_u32::<LittleEndian>()?,
        wav_entries_ptr: cur.read_u32::<LittleEndian>()?,
        wav_entries_size: cur.read_u32::<LittleEndian>()?,
        unknown_count_2: cur.read_u32::<LittleEndian>()?,
        unknown_1: cur.read_u32::<LittleEndian>()?,
        unknown_2: cur.read_u32::<LittleEndian>()?,
        wave_data_ptr: cur.read_u32::<LittleEndian>()?,
        wave_data_length: cur.read_u32::<LittleEndian>()?,
    };

    let num_wav_entries = header.wav_entries_size / (6 * 4);

    let mut wav_files: Vec<WavFile> = vec![];

    let mut raw_wav_entries = vec![RawWavEntry::default(); num_wav_entries as usize];

    // Read wav entries
    if num_wav_entries != 0 {
        cur.seek(SeekFrom::Start(header.wav_entries_ptr as u64))?;

        for i in 0..num_wav_entries as usize {
            let raw_entry = RawWavEntry {
                unknown_1: cur.read_u32::<LittleEndian>()?,
                flag1: cur.read_u8()?,
                flag2: cur.read_u8()?,
                flag3: cur.read_u8()?,
                flag4: cur.read_u8()?,
                bytes_ptr: cur.read_u32::<LittleEndian>()?,
                num_bytes: cur.read_u32::<LittleEndian>()?,
                unknown_2: cur.read_u32::<LittleEndian>()?,
                unknown_3: cur.read_u32::<LittleEndian>()?,
            };

            raw_wav_entries[i] = raw_entry;
        }
    }

    wav_files.resize(raw_wav_entries.len(), Default::default());

    // Read wav data
    let mut res_cursor = cur.clone();

    for (i, raw_entry) in raw_wav_entries.into_iter().enumerate() {
        let mut audio_bytes = vec![0u8; raw_entry.num_bytes as usize];

        res_cursor.seek(SeekFrom::Start(
            (raw_entry.bytes_ptr + header.wave_data_ptr) as u64,
        ))?;

        res_cursor.read_exact(&mut audio_bytes)?;

        wav_files[i] = WavFile::from_raw(raw_entry, audio_bytes);
    }

    dbg!(wav_files.len());

    Ok(())
}

const XWAVEBANK_HEADER_SIZE: usize = 40;

#[derive(Debug, Deserialize)]
#[repr(C, packed)]
pub(crate) struct XWavebankHeader {
    /// String which just says "WBND" in ASCII
    wbnd_string: [u8; 4],

    unknown_count_1: u32,

    header_size: u32, // Size of a WavebankHeader
    wavebanks_ptr: u32,

    wav_entries_ptr: u32,
    wav_entries_size: u32, // Total size of all the wav entries in bytes

    unknown_count_2: u32,

    unknown_1: u32,
    unknown_2: u32,

    wave_data_ptr: u32,
    wave_data_length: u32,
}

pub(crate) struct Wavebank {
    id: u32,
    num_entries: u32,
    name: [char; 16],
    idk1: u32,
    idk2: u32,
    num_or_ptr: u32,
    idk3: u32,
}

const RAW_WAV_ENTRY_SIZE: usize = 5 * size_of::<u32>() + 4 * size_of::<u8>();

#[derive(Debug, Deserialize, Default, Clone)]
pub(crate) struct RawWavEntry {
    unknown_1: u32,

    flag1: u8,
    flag2: u8,
    flag3: u8,
    flag4: u8,

    bytes_ptr: u32,
    num_bytes: u32,
    unknown_2: u32,
    unknown_3: u32,
}

#[derive(Default, Clone)]
pub struct WavFile {
    unknown_1: u32,

    flag1: u8,
    flag2: u8,
    flag3: u8,
    flag4: u8,

    bytes: Vec<u8>,

    unknown_2: u32,
    unknown_3: u32,
}

impl WavFile {
    pub(crate) fn from_raw(raw: RawWavEntry, bytes: Vec<u8>) -> Self {
        Self {
            unknown_1: raw.unknown_1,
            flag1: raw.flag1,
            flag2: raw.flag2,
            flag3: raw.flag3,
            flag4: raw.flag4,
            bytes,
            unknown_2: raw.unknown_2,
            unknown_3: raw.unknown_3,
        }
    }
}
