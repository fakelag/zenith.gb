pub mod apu;
mod channel1;
mod channel2;
mod channel3;
mod channel4;
mod lengthcounter;
mod envelope;
mod sweep;
mod wav_file;

pub trait Channel {
    fn step(&mut self);
    fn get_sample(&self) -> u8;
    fn get_length_counter(&mut self) -> &mut lengthcounter::LengthCounter;
    fn get_envelope(&mut self) -> Option<&mut envelope::Envelope>;
    fn is_enabled(&self) -> bool;
}
