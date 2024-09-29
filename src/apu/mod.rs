pub mod apu;
mod channel3;
mod lengthcounter;

pub trait Channel {
    fn step(&mut self);
    fn get_sample(&self) -> u8;
    fn get_length_counter(&mut self) -> &mut lengthcounter::LengthCounter;
    fn is_enabled(&self) -> bool;
}
