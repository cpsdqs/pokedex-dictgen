use crate::fetcher::Fetcher;
use anyhow::{bail, Context};
use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::data::{
    CFDataCreateMutable, CFDataGetBytePtr, CFDataGetLength, CFMutableDataRef,
};
use core_foundation::dictionary::{
    kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionaryCreate,
    CFDictionaryRef,
};
use core_foundation::number::{kCFNumberCGFloatType, CFNumberCreate};
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::base::{kCGRenderingIntentDefault, CGFloat};
use core_graphics::color_space::{kCGColorSpaceSRGB, CGColorSpace};
use core_graphics::data_provider::CGDataProvider;
use core_graphics::image::CGImage;
use core_graphics::image::CGImageAlphaInfo::CGImageAlphaLast;
use foreign_types::ForeignType;
use image::codecs::png::PngDecoder;
use image::{DynamicImage, ImageDecoder};
use std::path::PathBuf;
use std::sync::Arc;
use std::{fs, ptr};
use url::Url;

pub struct ImageCache {
    dir: PathBuf,
}

fn get_image_id_ext(url: &Url) -> anyhow::Result<(String, String)> {
    let path = url
        .path()
        .trim_start_matches("/media/upload")
        .trim_start_matches('/');

    let Some((name, ext)) = path.rsplit_once('.') else {
        bail!("image URL has no file extension: {url}");
    };
    let mut parts: Vec<_> = name.split('/').collect();
    parts.reverse();
    Ok((parts.join("-"), ext.to_string()))
}

const COMPRESSED_EXT: &str = "heif";

impl ImageCache {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn get(&self, fetcher: &Fetcher, url: &Url) -> anyhow::Result<String> {
        let (id, ext) = get_image_id_ext(url)?;

        let cache_path_ext = self.dir.join(format!("{id}.{ext}"));
        let cache_path_compressed = self.dir.join(format!("{id}.{COMPRESSED_EXT}"));

        if cache_path_compressed.exists() {
            Ok(format!("{id}.{COMPRESSED_EXT}"))
        } else if cache_path_ext.exists() {
            Ok(format!("{id}.{ext}"))
        } else {
            let data = fetcher
                .get(url.as_ref(), false)
                .context("error loading image")?;
            if let Some(compressed) =
                try_compress(&ext, &data).context("error compressing image")?
            {
                fs::write(cache_path_compressed, compressed)?;
                Ok(format!("{id}.{COMPRESSED_EXT}"))
            } else {
                fs::write(cache_path_ext, &data)?;
                Ok(format!("{id}.{ext}"))
            }
        }
    }
}

#[allow(non_camel_case_types)]
type size_t = isize;

enum CGImageDestination {}
type CGImageDestinationRef = *mut CGImageDestination;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGColorSpaceCreateICCBased(
        components: size_t,
        range: *const CGFloat,
        profile: *const core_graphics::sys::CGDataProvider,
        alternate: *const core_graphics::sys::CGColorSpace,
    ) -> *mut core_graphics::sys::CGColorSpace;
}

#[link(name = "ImageIO", kind = "framework")]
extern "C" {
    static kCGImageDestinationLossyCompressionQuality: CFStringRef;
    fn CGImageDestinationCreateWithData(
        data: CFMutableDataRef,
        type_: CFStringRef,
        count: size_t,
        options: CFDictionaryRef,
    ) -> CGImageDestinationRef;
    fn CGImageDestinationAddImage(
        dest: CGImageDestinationRef,
        image: *const core_graphics::sys::CGImage,
        props: CFDictionaryRef,
    );
    fn CGImageDestinationFinalize(dest: CGImageDestinationRef) -> bool;
}

pub fn try_compress(file_ext: &str, image: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
    if file_ext != "png" {
        return Ok(None);
    }

    let mut png = PngDecoder::new(std::io::Cursor::new(image))?;
    if png.is_apng() {
        return Ok(None);
    }
    let icc_profile = png.icc_profile();

    let img = DynamicImage::from_decoder(png)?.into_rgba8();

    unsafe {
        let mut color_space = CGColorSpace::create_with_name(kCGColorSpaceSRGB).unwrap();
        if let Some(icc) = icc_profile {
            let range = [0., 1., 0., 1., 0., 1.];
            let data_provider = CGDataProvider::from_buffer(Arc::new(icc));

            // note: this will fail for some grayscale images, since those don't have 3 components
            // that's... fine, i guess
            let space = CGColorSpaceCreateICCBased(
                3,
                range.as_ptr(),
                data_provider.as_ref() as *const _ as _,
                ptr::null(),
            );

            if !space.is_null() {
                color_space = CGColorSpace::from_ptr(space);
            }
        }

        let mut pixels = Vec::new();
        pixels.resize((img.width() * img.height() * 4) as usize, 0);
        for y in 0..img.height() {
            for x in 0..img.width() {
                let pixel = img.get_pixel(x, y);

                let i = y as usize * img.width() as usize + x as usize;
                pixels[i * 4] = pixel.0[0];
                pixels[i * 4 + 1] = pixel.0[1];
                pixels[i * 4 + 2] = pixel.0[2];
                pixels[i * 4 + 3] = pixel.0[3];
            }
        }
        let provider = CGDataProvider::from_buffer(Arc::new(pixels));

        let cg_image = CGImage::new(
            img.width() as _,
            img.height() as _,
            8,
            32,
            (img.width() * 4) as _,
            &color_space,
            CGImageAlphaLast as _,
            &provider,
            false,
            kCGRenderingIntentDefault,
        );

        let out_data = CFDataCreateMutable(ptr::null(), 0);
        let dest_type = CFString::new("public.heic");
        let destination = CGImageDestinationCreateWithData(
            out_data,
            dest_type.as_concrete_TypeRef(),
            1,
            ptr::null(),
        );

        let keys: [CFStringRef; 1] = [kCGImageDestinationLossyCompressionQuality];
        let compression: CGFloat = 0.8;
        let compression = CFNumberCreate(
            ptr::null(),
            kCFNumberCGFloatType,
            &compression as *const _ as _,
        );
        let values: [CFTypeRef; 1] = [compression as _];
        let options = CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr() as _,
            values.as_ptr() as _,
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        CGImageDestinationAddImage(destination, cg_image.as_ref() as *const _ as _, options);
        if !CGImageDestinationFinalize(destination) {
            bail!("unknown error");
        }

        CFRelease(options as _);
        CFRelease(compression as _);

        let out_data_ptr = CFDataGetBytePtr(out_data);
        let out_data_len = CFDataGetLength(out_data);
        let out = std::slice::from_raw_parts(out_data_ptr, out_data_len as usize).to_vec();

        CFRelease(out_data as _);

        Ok(Some(out))
    }
}
