use html5ever::serialize::{AttrRef, Serializer, TraversalScope};
use html5ever::{namespace_url, ns};
use kuchikiki::NodeRef;
use std::fmt;
use std::io::{self, Write};

struct XhtmlSerializer<W> {
    out: W,
}

pub struct XhtmlEscaped<'a>(pub &'a str, pub bool);
impl<'a> fmt::Display for XhtmlEscaped<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let text = self.0;
        // from html5ever
        let attr_mode = self.1;
        for c in text.chars() {
            match c {
                '&' => write!(f, "&amp;"),
                '"' if attr_mode => write!(f, "&quot;"),
                '<' if !attr_mode => write!(f, "&lt;"),
                '>' if !attr_mode => write!(f, "&gt;"),
                c => write!(f, "{}", c),
            }?;
        }
        Ok(())
    }
}

impl<W: Write> XhtmlSerializer<W> {
    fn write_escaped(&mut self, text: &str, attr_mode: bool) -> io::Result<()> {
        write!(self.out, "{}", XhtmlEscaped(text, attr_mode))
    }
}

impl<W: Write> Serializer for XhtmlSerializer<W> {
    fn start_elem<'a, AttrIter>(
        &mut self,
        name: html5ever::QualName,
        attrs: AttrIter,
    ) -> io::Result<()>
    where
        AttrIter: Iterator<Item = AttrRef<'a>>,
    {
        if name.ns != ns!(html) {
            panic!("unexpected namespace in {name:?}");
        }

        write!(self.out, "<{}", name.local)?;
        for (name, value) in attrs {
            if !name.ns.is_empty() && name.ns != ns!(html) {
                panic!("unexpected attr namespace in {name:?}");
            }

            write!(self.out, " {}=\"", name.local)?;
            self.write_escaped(value, true)?;
            write!(self.out, "\"")?;
        }
        write!(self.out, ">")?;

        Ok(())
    }

    fn end_elem(&mut self, name: html5ever::QualName) -> io::Result<()> {
        write!(self.out, "</{}>", name.local)
    }

    fn write_text(&mut self, text: &str) -> io::Result<()> {
        self.write_escaped(text, false)
    }

    fn write_comment(&mut self, text: &str) -> io::Result<()> {
        write!(self.out, "<!--{text}-->")
    }

    fn write_doctype(&mut self, name: &str) -> io::Result<()> {
        write!(self.out, "<!DOCTYPE {name}>")
    }

    fn write_processing_instruction(&mut self, target: &str, data: &str) -> io::Result<()> {
        write!(self.out, "<?{target} {data}>")
    }
}

pub fn serialize<W: Write>(out: &mut W, node: &NodeRef) -> io::Result<()> {
    let mut ser = XhtmlSerializer { out };
    html5ever::serialize::Serialize::serialize(node, &mut ser, TraversalScope::IncludeNode)
}
