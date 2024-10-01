use std::{io::Write, fs};

pub fn write_wav(file_name: &str, wav_data: &Vec<i16>) {
    let mut wav_file = Vec::new();

    let num_samples = wav_data.len() / 2;
    let file_size = (num_samples * 4 + 36) as u32;

    // [Master RIFF chunk]
    wav_file.write("RIFF".as_bytes()).unwrap();
    wav_file.write(&[((file_size >> 0) & 0xFF) as u8]).unwrap();
    wav_file.write(&[((file_size >> 8) & 0xFF) as u8]).unwrap();
    wav_file.write(&[((file_size >> 16) & 0xFF) as u8]).unwrap();
    wav_file.write(&[((file_size >> 24) & 0xFF) as u8]).unwrap();


    wav_file.write("WAVE".as_bytes()).unwrap();

    // [Chunk describing the data format]
    wav_file.write("fmt ".as_bytes()).unwrap();

    // format size
    wav_file.write(&[0x10]).unwrap();
    wav_file.write(&[0]).unwrap();
    wav_file.write(&[0]).unwrap();
    wav_file.write(&[0]).unwrap();

    // format PCM
    wav_file.write(&[1]).unwrap();
    wav_file.write(&[0]).unwrap();

    // Num Channels
    wav_file.write(&[2]).unwrap();
    wav_file.write(&[0]).unwrap();

    // rate=44100
    wav_file.write(&[0x44]).unwrap();
    wav_file.write(&[0xAC]).unwrap();
    wav_file.write(&[0]).unwrap();
    wav_file.write(&[0]).unwrap();

    // rate*2*2
    wav_file.write(&[0x10]).unwrap();
    wav_file.write(&[0xB1]).unwrap();
    wav_file.write(&[0x2]).unwrap();
    wav_file.write(&[0]).unwrap();

    // bytes per sample
    wav_file.write(&[4]).unwrap();
    wav_file.write(&[0]).unwrap();

    // bits per sample
    wav_file.write(&[0x10]).unwrap();
    wav_file.write(&[0]).unwrap();

    // [Chunk containing the sampled data]
    wav_file.write("data".as_bytes()).unwrap();
    let data_size = num_samples * 4;
    wav_file.write(&[((data_size >> 0) & 0xFF) as u8]).unwrap();
    wav_file.write(&[((data_size >> 8) & 0xFF) as u8]).unwrap();
    wav_file.write(&[((data_size >> 16) & 0xFF) as u8]).unwrap();
    wav_file.write(&[((data_size >> 24) & 0xFF) as u8]).unwrap();

    for s in wav_data {
        wav_file.write(&[(*s & 0xFF) as u8]).unwrap();
        wav_file.write(&[(*s >> 8) as u8]).unwrap();
    }

    fs::write("dev/test.wav", wav_file).unwrap();
    println!("Wrote {} to disk", file_name)
}
