use quick_xml::events::Event;
use quick_xml::Reader;
use serde::ser::{SerializeMap, SerializeSeq};
use crate::ConversionError;

#[derive(Debug)]
pub enum JNode {
    Obj(Vec<(String, JNode)>),
    Arr(Vec<JNode>),
    Str(String),
}

/// Thin wrapper so we can use serde's Serializer to emit JNode without going via Value.
pub struct JsonNodeSerializer<'a>(pub &'a JNode);

impl<'a> serde::Serialize for JsonNodeSerializer<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.0 {
            JNode::Str(s) => serializer.serialize_str(s),
            JNode::Arr(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(&JsonNodeSerializer(item))?;
                }
                seq.end()
            }
            JNode::Obj(fields) => {
                let mut map = serializer.serialize_map(Some(fields.len()))?;
                for (k, v) in fields {
                    map.serialize_entry(k, &JsonNodeSerializer(v))?;
                }
                map.end()
            }
        }
    }
}

/// Per-element builder: attributes + children + text → JNode
pub struct ElemNode {
    attrs: Vec<(String, JNode)>,
    children: Vec<(String, JNode)>,
    text: String,
}

impl ElemNode {
    fn new() -> Self {
        Self {
            attrs: Vec::new(),
            children: Vec::new(),
            text: String::new(),
        }
    }

    fn to_jnode(mut self) -> JNode {
        // Decide representation based on attrs/children/text
        let has_attrs = !self.attrs.is_empty();
        let has_children = !self.children.is_empty();
        let text_trimmed = self.text.trim();
        let has_text = !text_trimmed.is_empty();

        if !has_attrs && !has_children {
            // Only text -> plain string
            return JNode::Str(text_trimmed.to_string());
        }

        // Build object
        let mut fields: Vec<(String, JNode)> = Vec::new();

        // Attributes as "@attr"
        fields.extend(self.attrs.into_iter());

        // Group children by name; multiple -> array
        // Small n, so a simple O(n^2) grouping is fine
        while let Some((name, node)) = self.children.pop() {
            // Look for existing entry with same name
            if let Some((_, existing)) = fields.iter_mut().find(|(k, _)| *k == name) {
                match existing {
                    JNode::Arr(ref mut arr) => {
                        arr.insert(0, node); // preserve order (current at front, older after)
                    }
                    other => {
                        // Convert single -> array
                        let prev = std::mem::replace(other, JNode::Arr(Vec::new()));
                        if let JNode::Arr(ref mut arr) = other {
                            arr.push(node);
                            arr.insert(0, prev);
                        }
                    }
                }
            } else {
                // No existing field; add as single
                fields.push((name, node));
            }
        }

        // Text content as "$text" when mixed with attrs/children
        if has_text {
            fields.push((
                "$text".to_string(),
                JNode::Str(text_trimmed.to_string()),
            ));
        }

        JNode::Obj(fields)
    }
}

pub fn parse_xml_message_to_jnode(xml_bytes: &[u8]) -> Result<JNode, ConversionError> {
    let mut reader = Reader::from_reader(xml_bytes);

    let mut buf = Vec::new();
    let mut stack: Vec<ElemNode> = Vec::new();
    let mut current_name_stack: Vec<String> = Vec::new();
    let mut root: Option<JNode> = None;

    loop {
        buf.clear();
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|e| ConversionError::XmlParseError(e.to_string()))?;

        match event {
            Event::Eof => break,

            Event::Start(e) => {
                // New element
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let mut elem = ElemNode::new();

                // Attributes → "@attr"
                for attr in e.attributes() {
                    let attr = attr.map_err(|e| ConversionError::XmlParseError(e.to_string()))?;
                    let key = format!(
                        "@{}",
                        String::from_utf8_lossy(attr.key.as_ref()).to_string()
                    );
                    let value = attr
                        .unescape_value()
                        .map_err(|e| ConversionError::XmlParseError(e.to_string()))?
                        .into_owned();
                    elem.attrs.push((key, JNode::Str(value)));
                }

                stack.push(elem);
                current_name_stack.push(name);
            }

            Event::Empty(e) => {
                // Self-closing tag: treat as Start+End with no text/children
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let mut elem = ElemNode::new();

                for attr in e.attributes() {
                    let attr = attr.map_err(|e| ConversionError::XmlParseError(e.to_string()))?;
                    let key = format!(
                        "@{}",
                        String::from_utf8_lossy(attr.key.as_ref()).to_string()
                    );
                    let value = attr
                        .unescape_value()
                        .map_err(|e| ConversionError::XmlParseError(e.to_string()))?
                        .into_owned();
                    elem.attrs.push((key, JNode::Str(value)));
                }

                let node = elem.to_jnode();

                if let Some(parent) = stack.last_mut() {
                    parent.children.push((name, node));
                } else {
                    // Empty root element (degenerate case)
                    root = Some(node);
                }
            }

            Event::Text(e) => {
                if let Some(current) = stack.last_mut() {
                        let txt = e
                            .unescape()
                            .map_err(|e| ConversionError::XmlParseError(e.to_string()))?
                            .to_string();
                    current.text.push_str(&txt);
                }
            }

            Event::End(e) => {
                let end_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let name = current_name_stack
                    .pop()
                    .unwrap_or_else(|| end_name.clone());

                let elem = stack
                    .pop()
                    .ok_or_else(|| ConversionError::XmlParseError("Unbalanced XML".to_string()))?;

                let node = elem.to_jnode();

                if let Some(parent) = stack.last_mut() {
                    parent.children.push((name, node));
                } else {
                    // Finished root element for this message
                    root = Some(node);
                }
            }

            Event::CData(e) => {
                if let Some(current) = stack.last_mut() {
                    // CDATA is raw character data; just decode bytes
                    let txt = String::from_utf8_lossy(e.as_ref());
                    current.text.push_str(&txt);
                }
            }

            _ => {}
        }
    }

    root.ok_or_else(|| {
        ConversionError::XmlParseError("No root element found in message".to_string())
    })
}

