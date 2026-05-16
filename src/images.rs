use crate::d3d::D3DFormat;

pub fn transcode(
    width: usize,
    height: usize,
    src_format: D3DFormat,
    dst_format: D3DFormat,
    bytes: &[u8],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if src_format == dst_format {
        return Ok(bytes.to_vec().to_owned());
    }

    match src_format {
        D3DFormat::DXT1 | D3DFormat::DXT2_3 | D3DFormat::DXT4_5 => match dst_format {
            D3DFormat::RGBA8 | D3DFormat::ARGB8 => bcndecode::decode(
                bytes,
                width,
                height,
                if src_format == D3DFormat::DXT1 {
                    bcndecode::BcnEncoding::Bc1
                } else if src_format == D3DFormat::DXT2_3 {
                    bcndecode::BcnEncoding::Bc2
                } else {
                    bcndecode::BcnEncoding::Bc4
                },
                if dst_format == D3DFormat::RGBA8 {
                    bcndecode::BcnDecoderFormat::RGBA
                } else {
                    bcndecode::BcnDecoderFormat::ARGB
                },
            )
            .map_err(|e| e.into()),
            D3DFormat::DXT1 | D3DFormat::DXT2_3 | D3DFormat::DXT4_5 => {
                Err("unable to convert from DXT to DXT".into())
            }
        },
        D3DFormat::RGBA8 | D3DFormat::ARGB8 => match dst_format {
            D3DFormat::DXT1 | D3DFormat::DXT2_3 | D3DFormat::DXT4_5 => {
                if dst_format == D3DFormat::ARGB8 {
                    panic!("argb8 not implemented");
                }
                /*
                let no_alpha = if src_format == D3DFormat::RGBA8 {
                    remove_xxxa8_alpha(bytes)
                } else {
                    remove_axxx8_alpha(bytes)
                };
                */

                // Estimate 4 bits per pixel, don't account for width/height not a multiple of 4
                let mut out_bytes = Vec::with_capacity(width * height / 2);

                let pixels = bytes
                    .chunks_exact(4)
                    .map(|chunk| {
                        let (r, g, b) = (chunk[0], chunk[1], chunk[2]);
                        jkl::math::Rgb32F::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
                    })
                    .collect::<Vec<_>>();

                let get_pixel = |x, y| {
                    pixels
                        .get(y * width + x)
                        .copied()
                        .unwrap_or(jkl::math::Rgb32F::new(0.0, 0.0, 0.0))
                };

                for y in 0..height / 4 {
                    for x in 0..width / 4 {
                        let x0y0 = get_pixel(x * 4, y * 4);
                        let x0y1 = get_pixel(x * 4, y * 4 + 1);
                        let x0y2 = get_pixel(x * 4, y * 4 + 2);
                        let x0y3 = get_pixel(x * 4, y * 4 + 3);

                        let x1y0 = get_pixel(x * 4 + 1, y * 4);
                        let x1y1 = get_pixel(x * 4 + 1, y * 4 + 1);
                        let x1y2 = get_pixel(x * 4 + 1, y * 4 + 2);
                        let x1y3 = get_pixel(x * 4 + 1, y * 4 + 3);

                        let x2y0 = get_pixel(x * 4 + 2, y * 4);
                        let x2y1 = get_pixel(x * 4 + 2, y * 4 + 1);
                        let x2y2 = get_pixel(x * 4 + 2, y * 4 + 2);
                        let x2y3 = get_pixel(x * 4 + 2, y * 4 + 3);

                        let x3y0 = get_pixel(x * 4 + 3, y * 4);
                        let x3y1 = get_pixel(x * 4 + 3, y * 4 + 1);
                        let x3y2 = get_pixel(x * 4 + 3, y * 4 + 2);
                        let x3y3 = get_pixel(x * 4 + 3, y * 4 + 3);

                        out_bytes.extend_from_slice(
                            &jkl::image::block::bc1::Block::encode([
                                [x0y0, x0y1, x0y2, x0y3],
                                [x1y0, x1y1, x1y2, x1y3],
                                [x2y0, x2y1, x2y2, x2y3],
                                [x3y0, x3y1, x3y2, x3y3],
                            ])
                            .bytes(),
                        );
                    }
                }

                Ok(out_bytes)
            }
            D3DFormat::ARGB8 => todo!(),
            D3DFormat::RGBA8 => todo!(),
        },
    }
}

fn remove_xxxa8_alpha(img_bytes: &[u8]) -> Vec<u8> {
    let mut out_bytes = Vec::with_capacity(img_bytes.len() * 3 / 4);
    for chunk in img_bytes.chunks_exact(4) {
        out_bytes.push(chunk[0]);
        out_bytes.push(chunk[1]);
        out_bytes.push(chunk[2]);
    }

    out_bytes
}

fn remove_axxx8_alpha(img_bytes: &[u8]) -> Vec<u8> {
    let mut out_bytes = Vec::with_capacity(img_bytes.len() * 3 / 4);
    for chunk in img_bytes.chunks_exact(4) {
        out_bytes.push(chunk[1]);
        out_bytes.push(chunk[2]);
        out_bytes.push(chunk[3]);
    }

    out_bytes
}
