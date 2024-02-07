use crate::fetcher::Fetcher;
use crate::gen::generate_dictionary;
use crate::image::ImageCache;
use crate::index::read_index;
use crate::mon::read_mon;
use clap::Parser;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;

mod fetcher;
mod gen;
mod image;
mod index;
mod mon;
mod xhtml;

#[derive(Debug, Parser)]
struct Args {
    /// Will load high-resolution Pokémon images instead of just thumbnails.
    /// Enable this if you plan on zooming in.
    #[arg(long)]
    hq_pokemon_images: bool,
    /// How many body sections to load (“Biology,” “In the anime,” etc.).
    #[arg(long, default_value_t = 1)]
    max_body_sections: usize,
    /// Will load high-resolution body images instead of just thumbnails.
    /// Enable this if you plan on zooming in.
    #[arg(long)]
    hq_body_images: bool,
    /// Enables both HQ Pokémon images and HQ body images.
    #[arg(long)]
    hq: bool,
}

#[derive(Debug)]
pub struct Config {
    pub hq_pokemon_images: bool,
    pub hq_body_images: bool,
    pub max_body_sections: usize,
}

fn main() {
    let args = Args::parse();
    let config = Config {
        hq_pokemon_images: args.hq || args.hq_pokemon_images,
        hq_body_images: args.hq || args.hq_body_images,
        max_body_sections: args.max_body_sections,
    };

    fs::create_dir_all("data/fetch_cache").unwrap();
    fs::create_dir_all("data/images").unwrap();

    let fetcher = Arc::new(Fetcher::new("data/fetch_cache".into()));
    let images = Arc::new(ImageCache::new("data/images".into()));

    let index = read_index(&fetcher).unwrap_or_else(|e| {
        eprintln!("{e:#}");
        std::process::exit(1);
    });
    eprintln!("got {} entries", index.pokemon_pages.len());
    eprintln!("loading data");

    let pokemon: BTreeMap<_, _> = index
        .pokemon_pages
        .par_iter()
        .map(|(id, url)| {
            let mon = read_mon(&fetcher, &index, &images, &config, url).unwrap_or_else(|e| {
                eprintln!("error reading {id}: {e:#}");
                std::process::exit(1);
            });
            (*id, mon)
        })
        .collect();

    eprintln!("generating entries");

    let out = generate_dictionary(&pokemon).unwrap_or_else(|e| {
        eprintln!("error generating dictionary: {e:#}");
        std::process::exit(1);
    });
    fs::write("ddk/Dictionary.xml", out).unwrap();

    eprintln!("done!");
}
