use std::io::SeekFrom;

use binrw::BinReaderExt;

struct CueU32 {
    cue: u32,
}

struct CueTable {
    idk: u32,
}

#[binrw::binrw]
#[derive(Debug, Clone)]
pub struct CueEntry {
    // TODO: implement later
    pub flags: u16,
    pub sound_index: u16,
    pub name_ptr: u32,
    pub a: u32,
    pub b: u32,
    pub c: u32,
}

// enum SoundEventType: u8 {
// 	PLAY_COMPLEX = 0x01,
// 	ENVELOPE_AMPLITUDE = 0x0a,
// };
//
// struct SoundEvent {
// 	u8 eventType;
//
// 	if (eventType == SoundEventType::PLAY_COMPLEX) {
// 		u24 timestampMs;
// 		u8 paramSize;
// 		u8 flags;
// 		u16 loopCount;
//
// 		// if XSB_PLAY_EVENT_FLAG_COMPLEX
// 		if (flags & 0x4) {
// 			u8* offset: u32;
// 		} else {
// 			u16 soundIndex;
// 			u16 bankIndex;
// 		}
// 		u16 pitchVariationMin;
// 		u16 pitchVariationMax;
// 		u16 volumeVariationMin;
// 		u16 volumeVariationMax;
// 		u16 maxDelay;
// 		u16 idk1;
// 	} else if (eventType == SoundEventType::ENVELOPE_AMPLITUDE) {
// 		u8 bytes[0x13];
// 	}
// };
//
// struct ComplexSound {
// 	u8 eventCount;
// 	u24 eventsPtr;
//
// 	// SoundEvent events[eventCount] @ eventsPtr;
// 	SoundEvent events[1] @ eventsPtr;
//  }

#[repr(u8)]
pub enum XsbSoundFlag {
    /// XSB_SOUND_FLAG_TRIVIAL
    Trivial = 1 << 3,
    /// XSB_SOUND_FLAG_SIMPLE
    Simple = 1 << 4,
}

/*
   if (flags & XSB_SOUND_FLAG_TRIVIAL) {
       u16 waveIndex = entryU32;
       u16 wavebankIndex = entryU32 >> 16;
   }
   else if (flags & XSB_SOUND_FLAG_SIMPLE) {
       u32 simple @ entryU32;
   }
   else {
       ComplexSound complex @ entryU32;
   }
*/

#[expect(nonstandard_style)]
#[derive(Debug, Clone)]
#[binrw::binrw]
pub struct SoundEntryData {
    // u32 of entry resolved earlier
    volume: u16,
    pitch: u16,
    trackCount: u8,
    layer: u8,
    category: u8,
    flags: u8,
    parameters3DIndex: u16,
    priority: u8,
    i3dl2Volume: u8,
    eqGain: u16,
    eqQ: u16,
}

#[binrw::binrw]
#[derive(Debug, Clone)]
pub struct TrivialSound {
    /// Index of sound in referenced wavebank
    pub wave_index: u16,
    /// Index of referenced wavebank in XSoundbank::wavebank_entries
    pub wavebank_index: u16,
    pub data: SoundEntryData,
}

#[binrw::binrw]
#[derive(Debug, Clone)]
pub struct SimpleSound {
    pub ptr: u32,
    pub data: SoundEntryData,
}

#[binrw::binrw]
#[derive(Debug, Clone)]
pub struct ComplexSound {
    pub ptr: u32,
    pub data: SoundEntryData,
}

#[derive(Debug, Clone)]
pub enum SoundEntry {
    Trivial(TrivialSound),
    Simple(SimpleSound),
    Complex(ComplexSound),
}

impl binrw::BinRead for SoundEntry {
    type Args<'a> = ();

    fn read_options<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        _: binrw::Endian,
        _: Self::Args<'_>,
    ) -> binrw::BinResult<Self> {
        reader.seek_relative(11)?;
        let flags: u8 = reader.read_le()?;
        reader.seek_relative(-12)?;

        let ret = if flags & XsbSoundFlag::Trivial as u8 > 0 {
            Self::Trivial(reader.read_le()?)
        } else if flags & XsbSoundFlag::Simple as u8 > 0 {
            Self::Simple(reader.read_le()?)
        } else {
            Self::Complex(reader.read_le()?)
        };

        Ok(ret)
    }
}

/*
struct Sound {
    u32 flags;
    u32 idk1;
    float f1;
    float f2;
    float f3;
    float f4;
    // float f5;
};

struct SoundTable {
    u32 flags;
    u32 idk1;
    float soundFloats[5];

    u32 idk2;
    u32 idk3;
    u32 idk4;

    CueU32 cues[parent.numEntries1];
    Sound sounds[parent.numEntries2];
};
*/

#[binrw::binread]
#[derive(Debug, Clone)]
#[br(import(count: usize))]
#[expect(clippy::manual_non_exhaustive)]
pub struct WavebankArray {
    #[br(count = count)]
    pub names: Vec<super::WavebankName>,

    #[br(magic = b"Null")]
    _null: (),
}

#[binrw::binread]
#[derive(Debug, Clone)]
#[expect(clippy::manual_non_exhaustive)]
pub struct XSoundbank {
    #[brw(magic = b"SDBK")]
    _sdbk: (),

    #[br(assert(version == 11))]
    pub version: u16,

    pub crc: u16,
    #[br(temp)]
    wavebank_array_ptr: u32,

    pub cue_table_ptr: u32,

    // 0x10
    #[br(temp)]
    soundsPtr: u32,
    #[br(temp)]
    fileSize: u32,

    pub xsb_flags: u16,

    pub num_something: u16,

    #[br(temp)]
    num_sounds: u16,
    #[br(temp)]
    num_cues: u16,

    // 0x20
    pub num_entries4: u16,
    #[br(temp)]
    num_wavebanks_used: u16,

    pub short7: u16,
    pub short8: u16,
    pub name: super::SoundbankName,

    #[br(count = num_cues)]
    pub cue_entries: Vec<CueEntry>,
    #[br(count = num_sounds)]
    pub sound_entries: Vec<SoundEntry>,

    // Parsed fields
    #[br(args(num_wavebanks_used.into()), restore_position, seek_before = SeekFrom::Start(wavebank_array_ptr.into()))]
    pub wavebank_array: WavebankArray,
}
