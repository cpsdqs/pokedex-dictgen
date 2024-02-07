use crate::index::DexId;
use crate::mon::{MonEntry, MonImage};
use crate::xhtml::XhtmlEscaped;
use anyhow::Context;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

pub fn generate_dictionary(pokemon: &BTreeMap<DexId, MonEntry>) -> anyhow::Result<String> {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- generated file -->
<d:dictionary xmlns="http://www.w3.org/1999/xhtml" xmlns:d="http://www.apple.com/DTDs/DictionaryService-1.0.rng">
"#,
    );

    for (id, mon) in pokemon {
        generate_mon(&mut out, mon).with_context(|| format!("error generating entry {id}"))?;
    }

    write!(out, "</d:dictionary>")?;

    Ok(out)
}

fn text(s: &str) -> XhtmlEscaped {
    XhtmlEscaped(s, false)
}

fn attr(s: &str) -> XhtmlEscaped {
    XhtmlEscaped(s, true)
}

fn raw(s: &str) -> String {
    s
        // &nbsp; doesn't exist
        .replace("&nbsp;", "\u{00a0}")
        // the dictionary compiler eats spaces between elements for some reason
        .replace("</b> <i", "</b>\u{00a0}<wbr/><i")
        .replace("</a> <a", "</a>\u{00a0}<wbr/><a")
}

fn generate_mon(out: &mut String, mon: &MonEntry) -> anyhow::Result<()> {
    writeln!(
        out,
        r#"<d:entry id="pokemon-{}" d:title="{}">"#,
        mon.dex_id.0,
        attr(&mon.name),
    )?;

    let mut names_seen: BTreeSet<_> = [mon.name.clone(), mon.name_jp_text.clone()].into_iter().collect();
    writeln!(out, r#"<d:index d:value="{}" />"#, attr(&mon.name))?;
    writeln!(out, r#"<d:index d:value="{}" />"#, attr(&mon.name_jp_text))?;

    for (i, img) in mon.images.iter().enumerate() {
        if let Some(text) = img.caption_text.as_deref() {
            let name = if text.contains(&mon.name) {
                text.to_string()
            } else {
                // stuff like "Spring Form," which does not contain the name,
                // so we'll add it
                format!("{} - {text}", mon.name)
            };
            if names_seen.contains(&name) {
                continue;
            }
            writeln!(out, r#"<d:index d:value="{}" d:anchor="xpointer(//*[@id='pokemon-image-{}'])" />"#, attr(&name), i)?;
            names_seen.insert(name);
        }
    }

    writeln!(out, r#"<div class="outer-container">"#)?;
    writeln!(out, r#"<div class="pokedex-id">{}</div>"#, mon.dex_id)?;
    writeln!(out, r#"<h1 class="pokemon-name">{}</h1>"#, text(&mon.name))?;
    writeln!(out, r#"<ul class="pokemon-categories">"#)?;
    for category in &mon.categories_html {
        writeln!(out, r#"<li>{}</li>"#, raw(category))?;
    }
    writeln!(out, r#"</ul>"#)?;
    writeln!(
        out,
        r#"<div class="pokemon-name-jp">{} ({})</div>"#,
        raw(&mon.name_jp_html),
        raw(&mon.name_jp_translit_html)
    )?;

    writeln!(out, r#"<ul class="pokemon-images">"#)?;
    fn render_image(out: &mut String, image: &MonImage, i: usize) -> anyhow::Result<()> {
        writeln!(out, r#"<li class="pokemon-image" id="pokemon-image-{i}">"#)?;
        writeln!(
            out,
            r#"<img alt="{}" src="{}" style="width: {}px" />"#,
            attr(&image.alt),
            attr(&image.src),
            image.width
        )?;
        if let Some(caption) = &image.caption_html {
            writeln!(out, r#"<div class="image-caption">{}</div>"#, raw(caption))?;
        }
        writeln!(out, r#"</li>"#)?;
        Ok(())
    }

    let mut i = 0;
    while i < mon.images.len() {
        let image = &mon.images[i];
        if image.flex && mon.images.get(i + 1).map_or(false, |i| i.flex) {
            writeln!(out, r#"<li class="pokemon-images-flex"><ul>"#)?;
            while i < mon.images.len() && mon.images[i].flex {
                render_image(out, &mon.images[i], i)?;
                i += 1;
            }
            writeln!(out, r#"</ul></li>"#)?;
        } else {
            render_image(out, image, i)?;
            i += 1;
        }
    }
    writeln!(out, r#"</ul>"#)?;

    let info_box_style = mon
        .info_box_style
        .iter()
        .map(|(k, v)| format!("{k}:{v};"))
        .fold(String::new(), |s, prop| s + &prop);

    writeln!(
        out,
        r#"<table class="roundy top-info-box" style="{}"><tbody>"#,
        attr(&info_box_style)
    )?;
    for tr in &mon.top_info_boxes_html {
        writeln!(out, "{}", raw(tr))?;
    }
    writeln!(out, r#"</tbody></table>"#)?;

    writeln!(out, "{}", raw(&mon.summary_html))?;

    writeln!(
        out,
        r#"<table class="roundy extra-info-box" style="{}"><tbody>"#,
        attr(&info_box_style)
    )?;
    for tr in &mon.extra_info_boxes_html {
        writeln!(out, "{}", raw(tr))?;
    }
    writeln!(out, r#"</tbody></table>"#)?;

    writeln!(out, "{}", raw(&mon.body_html))?;

    writeln!(
        out,
        r#"<div class="footer-read-more"><a href="{}">Read more on Bulbapedia</a></div>"#,
        attr(&mon.url)
    )?;

    writeln!(out, r#"</div></d:entry>"#)?;

    Ok(())
}
