use crate::fetcher::Fetcher;
use crate::image::ImageCache;
use crate::index::DexId;
use crate::index::Index;
use crate::Config;
use anyhow::{anyhow, bail, ensure, Context};
use html5ever::tendril::TendrilSink;
use kuchikiki::NodeRef;
use std::collections::BTreeMap;
use url::Url;

const FIRST_EXTRA_INFO_BOX: &str = "Gender ratio";

#[derive(Debug)]
pub struct MonEntry {
    pub url: String,
    pub info_box_style: BTreeMap<String, String>,
    pub dex_id: DexId,
    pub name: String,
    pub categories_html: Vec<String>,
    pub name_jp_text: String,
    pub name_jp_html: String,
    pub name_jp_translit_html: String,
    pub images: Vec<MonImage>,
    pub top_info_boxes_html: Vec<String>,
    pub extra_info_boxes_html: Vec<String>,
    pub summary_html: String,
    pub body_html: String,
}

#[derive(Debug)]
pub struct MonImage {
    pub href: String,
    pub alt: String,
    pub width: u32,
    pub src: String,
    pub caption_text: Option<String>,
    pub caption_html: Option<String>,
    pub flex: bool,
}

pub fn read_mon(
    fetcher: &Fetcher,
    index: &Index,
    image_cache: &ImageCache,
    config: &Config,
    url: &str,
) -> anyhow::Result<MonEntry> {
    let html = String::from_utf8(fetcher.get(url, true)?)?;
    let doc = kuchikiki::parse_html().one(html);
    let base_url = Url::parse(url).unwrap();

    let Ok(info_box) = doc.select_first("table.roundy") else {
        bail!("could not find info box");
    };
    let info_box_style: BTreeMap<_, _> = get_attr(info_box.as_node(), "style")
        .map(|s| parse_simple_style_attr(&s))
        .unwrap_or_default()
        .into_iter()
        .filter(|(k, _)| ["background", "border", "padding", "text-align"].contains(&&**k))
        .collect();

    let mut header_box = None;
    let mut is_extra = false;
    let mut top_info_nodes = Vec::new();
    let mut extra_info_nodes = Vec::new();

    let info_box_tbody =
        first_el_child(info_box.as_node(), "tbody").ok_or(anyhow!("no info box tbody"))?;
    for tr in info_box_tbody.children().filter(is_element) {
        if header_box.is_none() {
            header_box = Some(tr);
            continue;
        }

        if tr.text_contents().trim().starts_with(FIRST_EXTRA_INFO_BOX) {
            is_extra = true;
        }
        if is_extra {
            extra_info_nodes.push(tr);
        } else {
            top_info_nodes.push(tr);
        }
    }

    let header_box = header_box.ok_or(anyhow!("no header box"))?;

    let (name, categories_html, name_jp_text, name_jp_html, name_jp_translit_html, dex_id, images) = {
        let td = first_el_child(&header_box, "td").ok_or(anyhow!("no header box > td"))?;
        let table = first_el_child(&td, "table").ok_or(anyhow!("no header box > td > table"))?;
        let tbody =
            first_el_child(&table, "tbody").ok_or(anyhow!("no header box > td > table > tbody"))?;
        let trs: Vec<_> = tbody.children().filter(is_element).collect();
        ensure!(trs.len() == 2, "unexpected header box tr count",);

        let first_tr_items: Vec<_> = trs[0].children().filter(is_element).collect();
        ensure!(
            first_tr_items.len() == 2,
            "unexpected header box tr > td count"
        );

        let name_box = &first_tr_items[0];
        let (name, categories_html, name_jp_text, name_jp_html, name_jp_translit_html) = {
            let table = first_el_child(name_box, "table").ok_or(anyhow!("no name box > table"))?;
            let tbody =
                first_el_child(&table, "tbody").ok_or(anyhow!("no name box > table > tbody"))?;
            let tr =
                first_el_child(&tbody, "tr").ok_or(anyhow!("no name box > table > tbody > tr"))?;
            let tds: Vec<_> = tr.children().filter(is_element).collect();
            ensure!(tds.len() == 2, "unexpected name box td count");

            let english_box = &tds[0];

            let big = english_box
                .select_first("big")
                .map_err(|()| anyhow!("missing name box <big>"))?;
            let name = big.text_contents().trim().to_string();

            let category_items = english_box
                .select_first("a[title]")
                .map_err(|()| anyhow!("missing name box categories"))?;
            let category_items: Vec<_> = category_items
                .as_node()
                .children()
                .filter(is_element)
                .collect();
            ensure!(
                category_items.len() == 1,
                "unexpected name box category item count"
            );
            let mut categories = Vec::new();
            for node in category_items[0].children() {
                if node
                    .as_element()
                    .map_or(false, |el| &*el.name.local == "br")
                {
                    categories.push(String::new());
                    continue;
                }
                let html = outer_xhtml(&node);
                if let Some(last) = categories.last_mut() {
                    last.push_str(&html);
                } else {
                    categories.push(html);
                }
            }
            if categories.last().map_or(false, |s| s.is_empty()) {
                categories.pop();
            }

            let jp_box = &tds[1];
            let name_jp = jp_box
                .select_first("[lang='ja']")
                .map_err(|()| anyhow!("could not find jp name"))?;
            let name_jp_text = name_jp.text_contents().trim().to_string();
            let name_jp_html = outer_xhtml(name_jp.as_node());

            let name_jp_translit = jp_box
                .select_first("i")
                .map_err(|()| anyhow!("could not find jp translit"))?;
            let name_jp_translit = outer_xhtml(name_jp_translit.as_node());

            (
                name,
                categories,
                name_jp_text,
                name_jp_html,
                name_jp_translit,
            )
        };

        let dex_id = first_tr_items[1]
            .select_first("a")
            .map_err(|()| anyhow!("could not find dex id"))?;
        let dex_id: DexId = dex_id.text_contents().trim().parse()?;

        let images = {
            let td = first_el_child(&trs[1], "td").ok_or(anyhow!("no img box > td"))?;
            let table = first_el_child(&td, "table").ok_or(anyhow!("no img box > td > table"))?;
            let tbody = first_el_child(&table, "tbody")
                .ok_or(anyhow!("no img box > td > table > tbody"))?;

            let mut images = Vec::new();

            for tr in tbody.children().filter(is_element) {
                let tr_style: BTreeMap<_, _> = get_attr(&tr, "style")
                    .map(|s| parse_simple_style_attr(&s))
                    .unwrap_or_default();

                if tr_style.get("display").map_or(false, |d| d == "none") {
                    continue;
                }

                let child_count = tr
                    .children()
                    .filter(is_element)
                    .filter(|td| {
                        let td_style: BTreeMap<_, _> = get_attr(td, "style")
                            .map(|s| parse_simple_style_attr(&s))
                            .unwrap_or_default();
                        td_style.get("display").map_or(true, |d| d != "none")
                    })
                    .count();

                for td in tr.children().filter(is_element) {
                    let td_style: BTreeMap<_, _> = get_attr(&td, "style")
                        .map(|s| parse_simple_style_attr(&s))
                        .unwrap_or_default();
                    if td_style.get("display").map_or(false, |d| d == "none") {
                        continue;
                    }

                    if let Ok(img) = td.select_first("img") {
                        let src = get_highest_quality_src(
                            img.as_node(),
                            &base_url,
                            config.hq_pokemon_images,
                        )
                        .ok_or(anyhow!("no img src"))?;
                        let image_id = image_cache.get(fetcher, &src)?;

                        let href = base_url.join(
                            &get_attr(&img.as_node().parent().unwrap(), "href").unwrap_or_default(),
                        )?;
                        let alt = get_attr(img.as_node(), "alt").unwrap_or_default();
                        let width = get_attr(img.as_node(), "width")
                            .unwrap_or_default()
                            .parse()
                            .context("error parsing img width")?;

                        let caption = td.select_first("small").ok().map(|caption| {
                            let text = caption.text_contents();
                            let html = inner_xhtml(caption.as_node());
                            (text, html)
                        });
                        let (caption_text, caption_html) =
                            caption.map_or((None, None), |(a, b)| (Some(a), Some(b)));

                        images.push(MonImage {
                            href: href.to_string(),
                            alt,
                            width,
                            src: format!("images/{}", urlencoding::encode(&image_id)),
                            caption_text,
                            caption_html,
                            flex: child_count > 1,
                        });
                    } else if !tr.text_contents().contains("Archives") {
                        bail!("unexpected img box child: {}", outer_xhtml(&tr));
                    }
                }
            }

            images
        };

        (
            name,
            categories_html,
            name_jp_text,
            name_jp_html,
            name_jp_translit_html,
            dex_id,
            images,
        )
    };

    for node in top_info_nodes.iter().chain(extra_info_nodes.iter()) {
        fix_links(fetcher, index, image_cache, config, &base_url, node)
            .context("error fixing info box links")?;
    }

    let top_info_boxes_html = top_info_nodes
        .into_iter()
        .map(|node| outer_xhtml(&node))
        .collect();
    let extra_info_boxes_html = extra_info_nodes
        .into_iter()
        .map(|node| outer_xhtml(&node))
        .collect();

    let mw_parser_output = doc
        .select_first(".mw-parser-output")
        .map_err(|()| anyhow!("no mw-parser-output"))?;
    let mut summary_nodes = Vec::new();
    let mut body_nodes = Vec::new();

    let mut tables_seen = 0;
    let mut h2s_seen = 0;
    let mut is_in_body = false;
    for node in mw_parser_output.as_node().children() {
        let tag_name = node.as_element().map(|el| &*el.name.local);
        let id = get_attr(&node, "id");
        if id.map_or(false, |id| id == "toc") {
            is_in_body = true;
            continue;
        }
        if tag_name == Some("table") {
            tables_seen += 1;
            if tables_seen < 3 {
                // skip the header and info box
                continue;
            }
        }
        if tag_name == Some("h2") {
            h2s_seen += 1;
        }
        if h2s_seen > config.max_body_sections {
            break;
        }
        fix_links(fetcher, index, image_cache, config, &base_url, &node)
            .context("error fixing summary links")?;
        if is_in_body {
            body_nodes.push(node);
        } else {
            summary_nodes.push(node);
        }
    }

    let summary_html = summary_nodes
        .into_iter()
        .fold(String::new(), |s, node| s + &outer_xhtml(&node));
    let body_html = body_nodes
        .into_iter()
        .fold(String::new(), |s, node| s + &outer_xhtml(&node));

    Ok(MonEntry {
        url: url.to_string(),
        info_box_style,
        dex_id,
        name,
        categories_html,
        name_jp_text,
        name_jp_html,
        name_jp_translit_html,
        images,
        top_info_boxes_html,
        extra_info_boxes_html,
        summary_html,
        body_html,
    })
}

fn fix_links(
    fetcher: &Fetcher,
    index: &Index,
    image_cache: &ImageCache,
    config: &Config,
    base_url: &Url,
    node: &NodeRef,
) -> anyhow::Result<()> {
    // remove references. we don't have a references section
    if let Ok(refs) = node.select("sup.reference") {
        for reference in refs.collect::<Vec<_>>() {
            reference.as_node().detach();
        }
    }

    if let Ok(links) = node.select("a") {
        for link in links {
            let mut attrs = link.as_node().as_element().unwrap().attributes.borrow_mut();
            if let Some(href) = attrs.get("href") {
                let url = base_url
                    .join(href)
                    .with_context(|| format!("error fixing <a href=\"{href}\""))?;
                let mut url_str = url.to_string();

                if href.starts_with("/wiki/") && href.ends_with("_(Pok%C3%A9mon)") {
                    if let Some((id, _)) = index
                        .pokemon_pages
                        .iter()
                        .find(|(_, page)| **page == *url_str)
                    {
                        url_str = format!("x-dictionary:r:pokemon-{}", id.0);
                        attrs.remove("title");
                    }
                }

                attrs.insert("href", url_str);
            }
        }
    }

    if let Ok(images) = node.select("img") {
        for image in images {
            let src = get_highest_quality_src(image.as_node(), base_url, config.hq_body_images)
                .ok_or(anyhow!("<img> without src"))?;
            let image_id = image_cache
                .get(fetcher, &src)
                .with_context(|| format!("error fixing <img src=\"{src}\">"))?;
            let mut attrs = image
                .as_node()
                .as_element()
                .unwrap()
                .attributes
                .borrow_mut();
            attrs.remove("srcset");
            attrs.insert("src", format!("images/{}", urlencoding::encode(&image_id)));

            // keep aspect ratio
            if attrs.contains("width") {
                attrs.remove("height");
            }
        }
    }

    Ok(())
}

fn get_attr(node: &NodeRef, attr: &str) -> Option<String> {
    let el = node.as_element()?;
    el.attributes.borrow().get(attr).map(|s| s.to_string())
}

fn first_el_child(node: &NodeRef, tag: &str) -> Option<NodeRef> {
    node.children()
        .find(|node| node.as_element().map_or(false, |el| &*el.name.local == tag))
}

fn is_element(node: &NodeRef) -> bool {
    node.as_element().is_some()
}

fn parse_simple_style_attr(style: &str) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    for entry in style.split(';') {
        let entry = entry.trim();
        if let Some((k, v)) = entry.split_once(':') {
            values.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    values
}

fn outer_xhtml(node: &NodeRef) -> String {
    let mut w = Vec::new();
    crate::xhtml::serialize(&mut w, node).unwrap();
    String::from_utf8(w).unwrap()
}

fn inner_xhtml(node: &NodeRef) -> String {
    let mut w = Vec::new();
    for child in node.children() {
        crate::xhtml::serialize(&mut w, &child).unwrap();
    }
    String::from_utf8(w).unwrap()
}

fn get_highest_quality_src(img: &NodeRef, base_url: &Url, find_thumb_origin: bool) -> Option<Url> {
    let mut src_set: BTreeMap<_, _> = get_attr(img, "srcset")
        .unwrap_or_default()
        .split(',')
        .filter_map(|entry| {
            let (src, size) = entry.rsplit_once(' ')?;
            Some((size.trim().to_string(), src.to_string()))
        })
        .collect();
    if !src_set.contains_key("1x") {
        src_set.insert("1x".to_string(), get_attr(img, "src").unwrap_or_default());
    }

    let src = src_set
        .get("2x")
        .or(src_set.get("1.5x"))
        .or(src_set.get("1x"))?;

    let src = base_url.join(src).ok()?;

    if find_thumb_origin {
        get_image_thumbnail_origin(&src).or(Some(src))
    } else {
        Some(src)
    }
}

fn get_image_thumbnail_origin(src: &Url) -> Option<Url> {
    if src.domain() != Some("archives.bulbagarden.net") || !src.path().contains("/thumb/") {
        return None;
    }
    let mut segments = src.path_segments().unwrap();
    if segments.next()? != "media" {
        return None;
    }
    if segments.next()? != "upload" {
        return None;
    }
    if segments.next()? != "thumb" {
        return None;
    }

    let a = segments.next()?;
    let b = segments.next()?;
    let file_name = segments.next()?;

    let new_path = format!("/media/upload/{a}/{b}/{file_name}");
    let mut url = src.clone();
    url.set_path(&new_path);
    Some(url)
}
