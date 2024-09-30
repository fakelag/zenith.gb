pub fn get_high(value: u16) -> u8 {
    (value >> 8) as u8
}

pub fn get_low(value: u16) -> u8 {
    (value & 0xFF) as u8
}

pub fn set_high(dst: &mut u16, value: u8) -> u16 {
    *dst = (*dst & 0xFF) | (u16::from(value) << 8);
    *dst
}

pub fn set_low(dst: &mut u16, value: u8) -> u16 {
    *dst = (*dst & 0xFF00) | u16::from(value);
    *dst
}

pub fn value(high: u8, low: u8) -> u16 {
    (u16::from(high) << 8) | u16::from(low)
}

pub fn audio_sample_u8_to_i16(sample: u8) -> i16 {
    (i16::from(sample) - 0) << 10
}
