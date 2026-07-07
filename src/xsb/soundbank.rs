use std::io::SeekFrom;

use binrw::BinReaderExt;

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

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct TrivialSound {
    pub bank_index: BankIndex, 
    pub data: SoundEntryData,
}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct SimpleSound {
    #[br(temp)]
    variations_ptr: u32,
    #[br(restore_position, seek_before = SeekFrom::Start(variations_ptr.into()))]
    pub variations: SimpleWaveVariations,
    pub data: SoundEntryData,
}

#[repr(u8)]
pub enum PlayEventBits {
    Complex = 0x04,
    LoopVariation = 0x40,
}

#[binrw::binrw]
#[brw(repr = u8)]
#[repr(u8)]
#[derive(num_enum::TryFromPrimitive)]
#[derive(Debug, Clone)]
pub enum ComplexEventType {
    Play = 0,
    PlayComplex = 1,
}

#[derive(Debug, Clone)]
pub struct VariationParams {
    pub flag1: bool,
    pub flag2: bool,
    pub current_variation: u16, // u13,
    pub variation_selection_method: u8, // u4
    pub num_variations: u16 // u13 
}

impl From<u32> for VariationParams {
    fn from(value: u32) -> Self {
        let flag1 = value & (1 << 31) > 0;
        let flag2 = value & (1 << 30) > 0;

        let current_variation = (value.unbounded_shr(17) as u16) & 0b1111111111111;
        let variation_selection_method = (value.unbounded_shr(13) as u8) & 0b1111;
        let num_variations  = (value as u16) & 0b1111111111111;

        Self {
            flag1,
            flag2,
            current_variation,
            variation_selection_method,
            num_variations
        }
    }
}


#[binrw::binread]
#[derive(Debug, Clone)]
pub struct SimpleWaveVariations {
    #[br(map = |x: [u8; 4]| u32::from_le_bytes(x).into())]
    pub variation_params: VariationParams, 
    #[br(count = variation_params.num_variations)]
    pub variations: Vec<BankIndex>
}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct ComplexWaveVariations {
    #[br(map = |x: [u8; 4]| u32::from_le_bytes(x).into())]
    pub variation_params: VariationParams, 
    #[br(count = variation_params.num_variations)]
    pub variations: Vec<ComplexVariation>
}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct BankIndex {
    /// Index of sound in referenced wavebank
    pub wave_index: u16,
    /// Index of referenced wavebank in XSoundbank::wavebank_entries
    pub wavebank_index: u16,
}


#[binrw::binread]
#[derive(Debug, Clone)]
pub struct ComplexVariation {
    pub bank_index: BankIndex,
    pub weight_min: u16,
    pub weight_max: u16,
}

#[binrw::binread]
#[derive(Debug, Clone)]
#[br(import(event_type: u8, flags: u8, params_size: u8))]
pub enum ComplexEventParams {
    #[br(assert(event_type == ComplexEventType::Play as u8 
            && ((flags & PlayEventBits::Complex as u8) == 0)))]
    Play(BankIndex),
    #[br(assert(event_type == ComplexEventType::PlayComplex as u8 
            && ((flags & PlayEventBits::Complex as u8) == 0)))]
    PlayComplex(BankIndex),
    #[br(assert(event_type == ComplexEventType::PlayComplex as u8 
            && ((flags & PlayEventBits::Complex as u8) != 0)))]
    PlayComplexWaveVariations {
        #[br(temp)]
        wave_variations_ptr: u32,

        #[br(restore_position, seek_before = SeekFrom::Start(wave_variations_ptr.into()))]
        wave_variations: ComplexWaveVariations
    },

    Unknown(
        /// event type
        u8,
        /// params
        #[br(count = params_size)]
        Vec<u8>,
    ),
}

pub fn from_u24_map(bytes: [u8; 3]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0])
}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct ComplexEvent {
    #[br(temp)]
    event_type: u8,
    // event type contained by ComplexEventParams
    #[br(map = from_u24_map)]
    pub timestamp_ms: u32,
    #[br(temp)]
    parameter_size: u8,
    pub flags: u8,
    pub unknown_u16: u16,
    #[br(args(event_type, flags, parameter_size))]
    pub params: ComplexEventParams,
}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct Complex {
    #[br(temp)]
    num_events: u8,

    #[br(temp, map = from_u24_map)]
    events_ptr: u32,

    #[br(count = num_events, restore_position, seek_before = SeekFrom::Start(events_ptr.into()))]
    pub events: Vec<ComplexEvent>,
}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct ComplexSound {
    #[br(temp)]
    pub complex_ptr: u32,
    #[br(restore_position, seek_before = SeekFrom::Start(complex_ptr.into()))]
    pub sound: Complex,
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
