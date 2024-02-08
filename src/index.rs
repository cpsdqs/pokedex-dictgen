use crate::fetcher::Fetcher;
use anyhow::{anyhow, bail};
use html5ever::tendril::TendrilSink;
use reqwest::Url;
use std::collections::BTreeMap;
use std::{fmt, str::FromStr};

const POKEMON_INDEX_URL: &str =
    "https://bulbapedia.bulbagarden.net/wiki/List_of_Pokémon_by_National_Pokédex_number";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DexId(pub u32);

impl FromStr for DexId {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s.strip_prefix('#').unwrap_or(s);
        Ok(Self(value.parse()?))
    }
}
impl TryFrom<String> for DexId {
    type Error = std::num::ParseIntError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl From<DexId> for String {
    fn from(value: DexId) -> Self {
        format!("{value}")
    }
}
impl fmt::Display for DexId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "#{:04}", self.0)
    }
}

impl DexId {
    pub fn prev(&self) -> Option<DexId> {
        if self.0 > 1 {
            Some(Self(self.0 - 1))
        } else {
            None
        }
    }
    pub fn next(&self) -> DexId {
        Self(self.0 + 1)
    }
}

#[derive(Debug)]
pub struct Index {
    pub pokemon_pages: BTreeMap<DexId, String>,
    pub pokemon_gens: Vec<Vec<DexId>>,
}

pub fn read_index(fetcher: &Fetcher) -> anyhow::Result<Index> {
    let html = String::from_utf8(fetcher.get(POKEMON_INDEX_URL, true)?)?;
    let doc = kuchikiki::parse_html().one(html);

    let base_url = Url::parse(POKEMON_INDEX_URL).unwrap();
    let mut pokemon_pages = BTreeMap::new();
    let mut pokemon_gens: Vec<Vec<_>> = Vec::new();

    for tr in doc
        .select("tr")
        .map_err(|()| anyhow!("could not find <tr>"))?
    {
        let tr = tr.as_node();

        let Ok(td) = tr.select_first("td") else {
            continue;
        };
        let Ok(dex_id) = td.text_contents().trim().parse() else {
            continue;
        };

        let generation = {
            let parent_table = td
                .as_node()
                .parent()
                .and_then(|node| node.parent())
                .and_then(|node| node.parent())
                .ok_or(anyhow!("entry is not in a table somehow"))?;

            let mut prev_sibling = parent_table.previous_sibling();
            while prev_sibling
                .as_ref()
                .map_or(false, |cursor| cursor.as_element().is_none())
            {
                prev_sibling = prev_sibling.unwrap().previous_sibling();
            }
            let gen_header = prev_sibling
                .as_ref()
                .and_then(|c| c.as_element())
                .ok_or(anyhow!("entry table is missing prev sibling"))?;
            if &*gen_header.name.local != "h3" {
                bail!("entry table does not come after a gen header");
            }
            let gen_title = prev_sibling.unwrap().text_contents();
            let gen_title = gen_title.trim();
            if !gen_title.starts_with("Generation ") {
                bail!("generation title does not start with “Generation”: {gen_title}");
            }
            let mut roman_numerals: Vec<_> = gen_title[11..]
                .chars()
                .map(|c| match c {
                    'I' => 1,
                    'V' => 5,
                    'X' => 10,
                    'L' => 50,
                    'C' => 100,
                    'D' => 500,
                    'M' => 1000,
                    _ => 0,
                })
                .filter(|i| *i != 0)
                .collect();

            for i in 0..roman_numerals.len() - 1 {
                if roman_numerals[i + 1] > roman_numerals[i] {
                    roman_numerals[i + 1] -= roman_numerals[i];
                    roman_numerals[i] = 0;
                }
            }
            roman_numerals.into_iter().sum()
        };
        if pokemon_gens.len() < generation {
            pokemon_gens.resize_with(generation, Default::default);
        }
        pokemon_gens[generation - 1].push(dex_id);

        let link = tr
            .select_first("a[href$='mon)']")
            .map_err(|()| anyhow!("missing link for entry {dex_id}"))?;
        let link_attrs = link.as_node().as_element().unwrap().attributes.borrow();
        let Some(href) = link_attrs.get("href") else {
            bail!("missing href on link for {dex_id}");
        };

        pokemon_pages.insert(dex_id, base_url.join(href).unwrap().to_string());
    }

    Ok(Index {
        pokemon_pages,
        pokemon_gens,
    })
}
