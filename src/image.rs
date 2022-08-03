use serde::Deserialize;

#[derive(Deserialize)]
pub struct Optimizations {
    quality: Option<u32>,
}

// let output_path = self.get_output_path(output_path)?;
//         println!("saving to {}...", output_path.display());
//         let mut file = File::create(&output_path)?;
//         let mut encoder = JpegEncoder::new_with_quality(&mut file, quality.unwrap_or(80));
//         encoder.encode_image(&DynamicImage::ImageRgba8(buffer))?;
