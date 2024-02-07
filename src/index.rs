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

#[derive(Debug)]
pub struct Index {
    pub pokemon_pages: BTreeMap<DexId, String>,
}

pub fn read_index(fetcher: &Fetcher) -> anyhow::Result<Index> {
    let html = String::from_utf8(fetcher.get(POKEMON_INDEX_URL, true)?)?;
    let doc = kuchikiki::parse_html().one(html);

    let base_url = Url::parse(POKEMON_INDEX_URL).unwrap();
    let mut pokemon_pages = BTreeMap::new();

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

        let link = tr
            .select_first("a[href$='mon)']")
            .map_err(|()| anyhow!("missing link for entry {dex_id}"))?;
        let link_attrs = link.as_node().as_element().unwrap().attributes.borrow();
        let Some(href) = link_attrs.get("href") else {
            bail!("missing href on link for {dex_id}");
        };

        pokemon_pages.insert(dex_id, base_url.join(href).unwrap().to_string());
    }

    Ok(Index { pokemon_pages })
}
