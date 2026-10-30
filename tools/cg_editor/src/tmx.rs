//! Tiled maps as the editor reads and writes them: the object layers of a `.tmx` file as
//! plain data, and saving them back into the file in place.
//!
//! Saving touches only the object layers that changed, and inside them only the objects
//! that changed: everything else (the tile data, tilesets, other layers, unchanged objects)
//! is copied from the file as it is, byte for byte, so Tiled still opens it and a version
//! control diff shows just the edit.

use core::fmt::Write as _;

/// An object layer of a map.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectLayer {
    /// The layer's id, unique in the map.
    pub id: u32,
    /// The layer's name.
    pub name: String,
    /// Its objects, in drawing order.
    pub objects: Vec<ObjectData>,
}

/// One object as the file stores it: Tiled pixels, y down, rotation clockwise in degrees.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectData {
    /// The object's id, unique in the map.
    pub id: u32,
    /// Its name.
    pub name: String,
    /// Its own class (the `type` attribute; `class` in files from Tiled 1.9). A tile
    /// object without one takes its tile's class.
    pub class: String,
    /// Whether the file used `class` rather than `type` for it.
    pub class_attribute: bool,
    /// The global tile id of a tile object, flip bits included; 0 for other objects.
    pub gid: u32,
    /// The anchor: the top-left corner of rectangles and ellipses, the bottom-left of tile
    /// objects, the point itself, the origin of polygons.
    pub x: f32,
    /// See [`x`](Self::x).
    pub y: f32,
    /// The width, 0 if the file gives none.
    pub width: f32,
    /// The height, 0 if the file gives none.
    pub height: f32,
    /// Clockwise rotation in degrees around the anchor.
    pub rotation: f32,
    /// Whether it is shown.
    pub visible: bool,
    /// The template file it is made from, if any.
    pub template: Option<String>,
    /// Its shape (tile objects and rectangles are both `Rect`).
    pub shape: Shape,
    /// Its own custom properties, in file order.
    pub properties: Vec<Property>,
    /// Attributes the editor does not know, kept as they are.
    pub extra_attributes: Vec<(String, String)>,
    /// Child elements the editor does not know, kept as they are.
    pub extra_children: Vec<String>,
}

impl ObjectData {
    /// A rectangle, tile or point object with no name, class or properties.
    pub fn new(id: u32, shape: Shape) -> Self {
        Self {
            id,
            name: String::new(),
            class: String::new(),
            class_attribute: false,
            gid: 0,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            rotation: 0.0,
            visible: true,
            template: None,
            shape,
            properties: Vec::new(),
            extra_attributes: Vec::new(),
            extra_children: Vec::new(),
        }
    }

    /// The property called `name`.
    pub fn property(&self, name: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.name == name)
    }
}

/// An object's shape.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// A rectangle, or a tile object.
    Rect,
    /// An ellipse.
    Ellipse,
    /// A point.
    Point,
    /// A closed polygon, its points as the file writes them (`"0,0 16,0 8,-12"`).
    Polygon(String),
    /// An open line, its points as the file writes them.
    Polyline(String),
    /// A text object: its `<text>` element as it is.
    Text(String),
}

impl Shape {
    /// The points of a polygon or polyline, relative to the anchor.
    pub fn points(&self) -> Vec<(f32, f32)> {
        let (Self::Polygon(points) | Self::Polyline(points)) = self else {
            return Vec::new();
        };
        points
            .split_whitespace()
            .filter_map(|pair| {
                let (x, y) = pair.split_once(',')?;
                Some((x.parse().ok()?, y.parse().ok()?))
            })
            .collect()
    }
}

/// A custom property.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    /// Its name.
    pub name: String,
    /// Its value.
    pub value: PropertyData,
}

/// A custom property's value, by Tiled's types.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyData {
    /// `string`.
    String(String),
    /// `int`.
    Int(i32),
    /// `float`.
    Float(f32),
    /// `bool`.
    Bool(bool),
    /// `color`, as the file writes it: `#aarrggbb`, or empty.
    Color(String),
    /// `file`, relative to the map as the file writes it.
    File(String),
    /// `object`: another object's id, 0 if unset.
    Object(u32),
    /// A custom class with its members.
    Class {
        /// The class's name (`propertytype`).
        class: String,
        /// Its members.
        members: Vec<Property>,
    },
    /// A kind the editor does not edit (a list, say): its `<property>` element as it is.
    Raw(String),
}

impl PropertyData {
    /// Tiled's name for the type.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::String(_) => "string",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Bool(_) => "bool",
            Self::Color(_) => "color",
            Self::File(_) => "file",
            Self::Object(_) => "object",
            Self::Class { .. } => "class",
            Self::Raw(_) => "other",
        }
    }
}

/// The object layers of a map file and what saving needs from it.
#[derive(Debug, Clone, PartialEq)]
pub struct MapObjects {
    /// The object layers, in file order (group layers flattened).
    pub layers: Vec<ObjectLayer>,
    /// The id the next new object gets (the map's `nextobjectid`).
    pub next_object_id: u32,
    /// Whether the file writes an object's children on its own line (as Tiled does) or
    /// all on the object's line.
    pub multiline: bool,
}

/// Reads the object layers of a `.tmx` file.
pub fn read_map(source: &str) -> Result<MapObjects, String> {
    let root = parse(source)?;
    if root.name != "map" {
        return Err(format!("the file is a `{}`, not a map", root.name));
    }
    let mut groups = Vec::new();
    object_groups(&root, &mut groups);
    let mut multiline = false;
    let mut layers = Vec::new();
    for group in groups {
        let mut objects = Vec::new();
        for child in group.children.iter().filter(|c| c.name == "object") {
            multiline |=
                !child.children.is_empty() && source[child.start..child.end].contains('\n');
            objects.push(read_object(source, child)?);
        }
        layers.push(ObjectLayer {
            id: group.attr("id").and_then(|id| id.parse().ok()).unwrap_or(0),
            name: group.attr("name").unwrap_or_default().to_owned(),
            objects,
        });
    }
    let highest = layers
        .iter()
        .flat_map(|layer| layer.objects.iter().map(|o| o.id))
        .max()
        .unwrap_or(0);
    let next_object_id = root
        .attr("nextobjectid")
        .and_then(|n| n.parse().ok())
        .unwrap_or(1)
        .max(highest + 1);
    Ok(MapObjects {
        layers,
        next_object_id,
        multiline,
    })
}

/// Writes `layers` into the map file `source` and returns the new file. Layers are matched
/// by id; an object layer of the file that `layers` lacks is left as it is, and a layer in
/// `layers` the file lacks is an error (the editor does not add layers; Tiled does).
pub fn write_map(source: &str, objects: &MapObjects) -> Result<String, String> {
    let root = parse(source)?;
    if root.name != "map" {
        return Err(format!("the file is a `{}`, not a map", root.name));
    }
    let mut groups = Vec::new();
    object_groups(&root, &mut groups);
    for layer in &objects.layers {
        if !groups
            .iter()
            .any(|g| g.attr("id") == Some(&layer.id.to_string()))
        {
            return Err(format!(
                "the file has no object layer {} (`{}`) any more",
                layer.id, layer.name
            ));
        }
    }
    // (start, end, replacement), in file order.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    if let Some(id) = root.attr("nextobjectid")
        && id.parse::<u32>().ok() != Some(objects.next_object_id)
        && let Some(edit) = replace_attribute(
            source,
            &root,
            "nextobjectid",
            &objects.next_object_id.to_string(),
        )
    {
        edits.push(edit);
    }
    for group in groups {
        let Some(layer) = objects
            .layers
            .iter()
            .find(|l| group.attr("id") == Some(&l.id.to_string()))
        else {
            continue;
        };
        let old: Vec<(&Element, ObjectData)> = group
            .children
            .iter()
            .filter(|c| c.name == "object")
            .map(|c| read_object(source, c).map(|o| (c, o)))
            .collect::<Result<_, _>>()?;
        let unchanged = old.len() == layer.objects.len()
            && old.iter().zip(&layer.objects).all(|((_, a), b)| a == b);
        if unchanged {
            continue;
        }
        edits.push((
            group.start,
            group.end,
            write_group(source, group, &old, layer, objects.multiline),
        ));
    }
    edits.sort_by_key(|(start, ..)| *start);
    let mut out = String::with_capacity(source.len() + 256);
    let mut at = 0;
    for (start, end, text) in edits {
        out.push_str(&source[at..start]);
        out.push_str(&text);
        at = end;
    }
    out.push_str(&source[at..]);
    Ok(out)
}

/// An object layer written again: its start tag and other children as they were, then the
/// objects, each copied from the file when it did not change.
fn write_group(
    source: &str,
    group: &Element,
    old: &[(&Element, ObjectData)],
    layer: &ObjectLayer,
    multiline: bool,
) -> String {
    let indent = indent_before(source, group.start);
    let inner = format!("{indent} ");
    let others: Vec<&Element> = group
        .children
        .iter()
        .filter(|c| c.name != "object")
        .collect();
    let mut out = String::new();
    let start_tag = &source[group.start..group.start_tag_end];
    if layer.objects.is_empty() && others.is_empty() {
        // Tiled writes empty layers as one tag.
        out.push_str(
            start_tag
                .trim_end_matches('>')
                .trim_end_matches('/')
                .trim_end(),
        );
        out.push_str("/>");
        return out;
    }
    if group.self_closing {
        out.push_str(
            start_tag
                .trim_end_matches('>')
                .trim_end_matches('/')
                .trim_end(),
        );
        out.push('>');
    } else {
        out.push_str(start_tag);
    }
    for other in others {
        out.push('\n');
        out.push_str(&inner);
        out.push_str(&source[other.start..other.end]);
    }
    for object in &layer.objects {
        out.push('\n');
        out.push_str(&inner);
        match old.iter().find(|(_, o)| o == object) {
            Some((element, _)) => out.push_str(&source[element.start..element.end]),
            None => write_object(&mut out, object, &inner, multiline),
        }
    }
    out.push('\n');
    out.push_str(&indent);
    out.push_str("</objectgroup>");
    out
}

/// The spaces and tabs between the line break before `at` and `at`.
fn indent_before(source: &str, at: usize) -> String {
    let line_start = source[..at].rfind('\n').map_or(0, |i| i + 1);
    let indent = &source[line_start..at];
    if indent.chars().all(|c| c == ' ' || c == '\t') {
        indent.to_owned()
    } else {
        " ".to_owned()
    }
}

/// Writes one object element, as Tiled does: attributes in Tiled's order, then the
/// properties and the shape, on their own lines when `multiline`.
fn write_object(out: &mut String, object: &ObjectData, indent: &str, multiline: bool) {
    out.push_str("<object");
    attribute(out, "id", &object.id.to_string());
    if let Some(template) = &object.template {
        attribute(out, "template", template);
    }
    if !object.name.is_empty() {
        attribute(out, "name", &object.name);
    }
    if !object.class.is_empty() {
        let key = if object.class_attribute {
            "class"
        } else {
            "type"
        };
        attribute(out, key, &object.class);
    }
    if object.gid != 0 {
        attribute(out, "gid", &object.gid.to_string());
    }
    attribute(out, "x", &number(object.x));
    attribute(out, "y", &number(object.y));
    if object.width != 0.0 || object.height != 0.0 {
        attribute(out, "width", &number(object.width));
        attribute(out, "height", &number(object.height));
    }
    if object.rotation != 0.0 {
        attribute(out, "rotation", &number(object.rotation));
    }
    if !object.visible {
        attribute(out, "visible", "0");
    }
    for (name, value) in &object.extra_attributes {
        attribute(out, name, value);
    }
    let mut children: Vec<String> = Vec::new();
    if !object.properties.is_empty() {
        let mut text = String::new();
        write_properties(
            &mut text,
            &object.properties,
            &format!("{indent} "),
            multiline,
        );
        children.push(text);
    }
    match &object.shape {
        Shape::Rect => {}
        Shape::Ellipse => children.push("<ellipse/>".into()),
        Shape::Point => children.push("<point/>".into()),
        Shape::Polygon(points) => {
            children.push(format!("<polygon points=\"{}\"/>", escape(points)))
        }
        Shape::Polyline(points) => {
            children.push(format!("<polyline points=\"{}\"/>", escape(points)))
        }
        Shape::Text(element) => children.push(element.clone()),
    }
    children.extend(object.extra_children.iter().cloned());
    if children.is_empty() {
        out.push_str("/>");
        return;
    }
    out.push('>');
    for child in children {
        if multiline {
            out.push('\n');
            out.push_str(indent);
            out.push(' ');
        }
        out.push_str(&child);
    }
    if multiline {
        out.push('\n');
        out.push_str(indent);
    }
    out.push_str("</object>");
}

fn write_properties(out: &mut String, properties: &[Property], indent: &str, multiline: bool) {
    out.push_str("<properties>");
    for property in properties {
        if multiline {
            out.push('\n');
            out.push_str(indent);
            out.push(' ');
        }
        write_property(out, property, &format!("{indent} "), multiline);
    }
    if multiline {
        out.push('\n');
        out.push_str(indent);
    }
    out.push_str("</properties>");
}

fn write_property(out: &mut String, property: &Property, indent: &str, multiline: bool) {
    let value = match &property.value {
        PropertyData::Raw(element) => {
            out.push_str(element);
            return;
        }
        PropertyData::Class { class, members } => {
            out.push_str("<property");
            attribute(out, "name", &property.name);
            attribute(out, "type", "class");
            attribute(out, "propertytype", class);
            if members.is_empty() {
                out.push_str("/>");
                return;
            }
            out.push('>');
            if multiline {
                out.push('\n');
                out.push_str(indent);
                out.push(' ');
            }
            write_properties(out, members, &format!("{indent} "), multiline);
            if multiline {
                out.push('\n');
                out.push_str(indent);
            }
            out.push_str("</property>");
            return;
        }
        PropertyData::String(text) if text.contains('\n') => {
            out.push_str("<property");
            attribute(out, "name", &property.name);
            out.push('>');
            out.push_str(&escape_text(text));
            out.push_str("</property>");
            return;
        }
        PropertyData::String(text) => text.clone(),
        PropertyData::Int(value) => value.to_string(),
        PropertyData::Float(value) => number(*value),
        PropertyData::Bool(value) => value.to_string(),
        PropertyData::Color(text) | PropertyData::File(text) => text.clone(),
        PropertyData::Object(id) => id.to_string(),
    };
    out.push_str("<property");
    attribute(out, "name", &property.name);
    if !matches!(property.value, PropertyData::String(_)) {
        attribute(out, "type", property.value.type_name());
    }
    attribute(out, "value", &value);
    out.push_str("/>");
}

fn attribute(out: &mut String, name: &str, value: &str) {
    let _ = write!(out, " {name}=\"{}\"", escape(value));
}

/// A number as Tiled writes it: whole numbers without a fraction.
fn number(value: f32) -> String {
    if value.fract() == 0.0 && value.abs() < 1e9 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            c => out.push(c),
        }
    }
    out
}

fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn replace_attribute(
    source: &str,
    element: &Element,
    name: &str,
    value: &str,
) -> Option<(usize, usize, String)> {
    let tag = &source[element.start..element.start_tag_end];
    let key = format!(" {name}=\"");
    let at = tag.find(&key)? + key.len();
    let end = tag[at..].find('"')? + at;
    Some((element.start + at, element.start + end, escape(value)))
}

/// The `objectgroup` elements under `element`, through group layers, in file order.
fn object_groups<'a>(element: &'a Element, out: &mut Vec<&'a Element>) {
    for child in &element.children {
        match child.name.as_str() {
            "objectgroup" => out.push(child),
            "group" => object_groups(child, out),
            _ => {}
        }
    }
}

fn read_object(source: &str, element: &Element) -> Result<ObjectData, String> {
    let number = |name: &str| -> Result<f32, String> {
        element.attr(name).map_or(Ok(0.0), |v| {
            v.parse()
                .map_err(|_| format!("object attribute `{name}` is not a number: `{v}`"))
        })
    };
    let id = element
        .attr("id")
        .and_then(|id| id.parse().ok())
        .ok_or("an object without an id")?;
    let mut object = ObjectData::new(id, Shape::Rect);
    for (name, value) in &element.attrs {
        match name.as_str() {
            "id" | "x" | "y" | "width" | "height" | "rotation" => {}
            "name" => object.name.clone_from(value),
            "type" => object.class.clone_from(value),
            "class" => {
                object.class.clone_from(value);
                object.class_attribute = true;
            }
            "gid" => {
                object.gid = value
                    .parse()
                    .map_err(|_| format!("object {id}: bad gid `{value}`"))?;
            }
            "visible" => object.visible = value != "0",
            "template" => object.template = Some(value.clone()),
            _ => object.extra_attributes.push((name.clone(), value.clone())),
        }
    }
    object.x = number("x")?;
    object.y = number("y")?;
    object.width = number("width")?;
    object.height = number("height")?;
    object.rotation = number("rotation")?;
    for child in &element.children {
        match child.name.as_str() {
            "properties" => object.properties = read_properties(source, child),
            "ellipse" => object.shape = Shape::Ellipse,
            "point" => object.shape = Shape::Point,
            "polygon" => {
                object.shape = Shape::Polygon(child.attr("points").unwrap_or_default().to_owned())
            }
            "polyline" => {
                object.shape = Shape::Polyline(child.attr("points").unwrap_or_default().to_owned())
            }
            "text" => object.shape = Shape::Text(source[child.start..child.end].to_owned()),
            _ => object
                .extra_children
                .push(source[child.start..child.end].to_owned()),
        }
    }
    Ok(object)
}

fn read_properties(source: &str, element: &Element) -> Vec<Property> {
    element
        .children
        .iter()
        .filter(|c| c.name == "property")
        .map(|c| read_property(source, c))
        .collect()
}

fn read_property(source: &str, element: &Element) -> Property {
    let name = element.attr("name").unwrap_or_default().to_owned();
    let raw = || PropertyData::Raw(source[element.start..element.end].to_owned());
    let text = || {
        element
            .attr("value")
            .map_or_else(|| element.text.clone(), str::to_owned)
    };
    let value = match element.attr("type").unwrap_or("string") {
        "string" => PropertyData::String(text()),
        "int" => text().parse().map_or_else(|_| raw(), PropertyData::Int),
        "float" => text().parse().map_or_else(|_| raw(), PropertyData::Float),
        "bool" => match text().as_str() {
            "true" => PropertyData::Bool(true),
            "false" => PropertyData::Bool(false),
            _ => raw(),
        },
        "color" => PropertyData::Color(text()),
        "file" => PropertyData::File(text()),
        "object" => text().parse().map_or_else(|_| raw(), PropertyData::Object),
        "class" => PropertyData::Class {
            class: element.attr("propertytype").unwrap_or_default().to_owned(),
            members: element
                .children
                .iter()
                .find(|c| c.name == "properties")
                .map(|p| read_properties(source, p))
                .unwrap_or_default(),
        },
        _ => raw(),
    };
    Property { name, value }
}

// --- A small XML reader --------------------------------------------------------------

/// An element of an XML file, with where it is in the text.
#[derive(Debug, Clone)]
struct Element {
    name: String,
    /// Attributes, their values unescaped.
    attrs: Vec<(String, String)>,
    /// The byte offset of its `<`.
    start: usize,
    /// Just after its start tag's `>`.
    start_tag_end: usize,
    /// Just after its end tag (or after the start tag's `/>`).
    end: usize,
    self_closing: bool,
    children: Vec<Element>,
    /// Its own text, unescaped.
    text: String,
}

impl Element {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Parses an XML file into its root element. Enough XML for Tiled's files: elements,
/// attributes, text, comments, CDATA, the declaration and a doctype.
fn parse(source: &str) -> Result<Element, String> {
    let bytes = source.as_bytes();
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut at = 0;
    let line_of = |at: usize| source[..at].matches('\n').count() + 1;
    while at < bytes.len() {
        let Some(lt) = source[at..].find('<').map(|i| i + at) else {
            break;
        };
        if let Some(open) = stack.last_mut() {
            open.text.push_str(&unescape(&source[at..lt]));
        }
        let rest = &source[lt..];
        if rest.starts_with("<?") {
            at = lt + rest.find("?>").ok_or("an unclosed `<?`")? + 2;
        } else if rest.starts_with("<!--") {
            at = lt + rest.find("-->").ok_or("an unclosed comment")? + 3;
        } else if rest.starts_with("<![CDATA[") {
            let end = rest.find("]]>").ok_or("an unclosed CDATA section")?;
            if let Some(open) = stack.last_mut() {
                open.text.push_str(&rest[9..end]);
            }
            at = lt + end + 3;
        } else if rest.starts_with("<!") {
            at = lt + rest.find('>').ok_or("an unclosed `<!`")? + 1;
        } else if let Some(close) = rest.strip_prefix("</") {
            let end = close.find('>').ok_or("an unclosed end tag")?;
            let name = close[..end].trim();
            let mut element = stack
                .pop()
                .ok_or_else(|| format!("line {}: `</{name}>` closes nothing", line_of(lt)))?;
            if element.name != name {
                return Err(format!(
                    "line {}: `</{name}>` closes `<{}>`",
                    line_of(lt),
                    element.name
                ));
            }
            element.end = lt + 2 + end + 1;
            at = element.end;
            match stack.last_mut() {
                Some(parent) => parent.children.push(element),
                None => root = Some(element),
            }
        } else {
            let (element, end) =
                start_tag(source, lt).map_err(|e| format!("line {}: {e}", line_of(lt)))?;
            at = end;
            if element.self_closing {
                match stack.last_mut() {
                    Some(parent) => parent.children.push(element),
                    None => root = Some(element),
                }
            } else {
                stack.push(element);
            }
        }
    }
    if let Some(open) = stack.last() {
        return Err(format!("`<{}>` is never closed", open.name));
    }
    root.ok_or_else(|| "the file has no elements".into())
}

/// Reads the start tag at `lt`; returns the element and the offset after the tag.
fn start_tag(source: &str, lt: usize) -> Result<(Element, usize), String> {
    let bytes = source.as_bytes();
    let mut at = lt + 1;
    let name_end = source[at..]
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .map(|i| i + at)
        .ok_or("an unclosed tag")?;
    let name = source[at..name_end].to_owned();
    at = name_end;
    let mut attrs = Vec::new();
    loop {
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        match bytes.get(at) {
            None => return Err(format!("`<{name}` is never closed")),
            Some(b'>') => {
                return Ok((element(name, attrs, lt, at + 1, false), at + 1));
            }
            Some(b'/') if bytes.get(at + 1) == Some(&b'>') => {
                return Ok((element(name, attrs, lt, at + 2, true), at + 2));
            }
            Some(_) => {
                let eq = source[at..]
                    .find('=')
                    .ok_or("an attribute without a value")?
                    + at;
                let key = source[at..eq].trim().to_owned();
                let mut quote_at = eq + 1;
                while bytes.get(quote_at).is_some_and(u8::is_ascii_whitespace) {
                    quote_at += 1;
                }
                let quote = *bytes.get(quote_at).ok_or("an attribute without a value")?;
                if quote != b'"' && quote != b'\'' {
                    return Err(format!("attribute `{key}` is not quoted"));
                }
                let value_end = source[quote_at + 1..]
                    .find(quote as char)
                    .ok_or_else(|| format!("attribute `{key}` is never closed"))?
                    + quote_at
                    + 1;
                attrs.push((key, unescape(&source[quote_at + 1..value_end])));
                at = value_end + 1;
            }
        }
    }
}

fn element(
    name: String,
    attrs: Vec<(String, String)>,
    start: usize,
    end: usize,
    self_closing: bool,
) -> Element {
    Element {
        name,
        attrs,
        start,
        start_tag_end: end,
        end,
        self_closing,
        children: Vec::new(),
        text: String::new(),
    }
}

fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest.find(';') else {
            break;
        };
        let entity = &rest[1..semi];
        let decoded = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<map version="1.10" orientation="orthogonal" width="4" height="3" tilewidth="8" tileheight="8" infinite="0" nextlayerid="6" nextobjectid="5">
 <tileset firstgid="1" source="../tilesets/tiles.tsx"/>
 <layer id="1" name="ground" width="4" height="3">
  <data encoding="csv">
0,0,0,0,
0,1,0,0,
2,2,2,2
</data>
 </layer>
 <!-- A comment with <tags> -->
 <group id="2" name="back">
  <objectgroup id="3" name="deco"/>
 </group>
 <objectgroup id="5" name="things">
  <properties>
   <property name="music" value="calm"/>
  </properties>
  <object id="1" name="coin &amp; more" type="Coin" x="8" y="0" width="8" height="8"/>
  <object id="2" gid="4" x="16" y="24" width="8" height="8"/>
  <object id="3" name="start" type="PlayerStart" x="4" y="12">
   <point/>
  </object>
  <object id="4" name="guard" x="0.5" y="0" width="4" height="2" rotation="45">
   <properties>
    <property name="Layer" type="int" value="7"/>
    <property name="Patrol" type="class" propertytype="Patrol">
     <properties>
      <property name="speed" type="float" value="40"/>
     </properties>
    </property>
    <property name="note">two
lines</property>
   </properties>
  </object>
 </objectgroup>
</map>
"#;

    #[test]
    fn reads_object_layers_through_groups() {
        let map = read_map(MAP).unwrap();
        assert_eq!(map.next_object_id, 5);
        assert!(map.multiline);
        let names: Vec<_> = map.layers.iter().map(|l| (l.id, l.name.as_str())).collect();
        assert_eq!(names, [(3, "deco"), (5, "things")]);
        let objects = &map.layers[1].objects;
        assert_eq!(objects[0].name, "coin & more");
        assert_eq!((objects[0].class.as_str(), objects[0].width), ("Coin", 8.0));
        assert_eq!(objects[1].gid, 4);
        assert_eq!(objects[2].shape, Shape::Point);
        let guard = &objects[3];
        assert_eq!((guard.x, guard.rotation), (0.5, 45.0));
        assert_eq!(guard.property("Layer").unwrap().value, PropertyData::Int(7));
        let PropertyData::Class { class, members } = &guard.property("Patrol").unwrap().value
        else {
            panic!("a class property");
        };
        assert_eq!(
            (class.as_str(), &members[0].value),
            ("Patrol", &PropertyData::Float(40.0))
        );
        assert_eq!(
            guard.property("note").unwrap().value,
            PropertyData::String("two\nlines".into())
        );
    }

    #[test]
    fn unchanged_maps_are_written_byte_for_byte() {
        let map = read_map(MAP).unwrap();
        assert_eq!(write_map(MAP, &map).unwrap(), MAP);
    }

    #[test]
    fn only_changed_objects_are_written_again() {
        let mut map = read_map(MAP).unwrap();
        map.layers[1].objects[1].x = 24.0;
        let saved = write_map(MAP, &map).unwrap();
        let changed: Vec<_> = MAP
            .lines()
            .zip(saved.lines())
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(
            changed,
            [(
                r#"  <object id="2" gid="4" x="16" y="24" width="8" height="8"/>"#,
                r#"  <object id="2" gid="4" x="24" y="24" width="8" height="8"/>"#
            )]
        );
        assert_eq!(read_map(&saved).unwrap(), map);
    }

    #[test]
    fn new_deleted_and_moved_objects_round_trip() {
        let mut map = read_map(MAP).unwrap();
        let things = &mut map.layers[1].objects;
        let guard = things.remove(3);
        things.remove(0);
        let mut spring = ObjectData::new(map.next_object_id, Shape::Rect);
        spring.gid = 2_147_483_652;
        (spring.x, spring.y, spring.width, spring.height) = (32.0, 24.0, 8.0, 8.0);
        spring.properties.push(Property {
            name: "power".into(),
            value: PropertyData::Float(1.5),
        });
        map.layers[1].objects.push(spring);
        map.next_object_id += 1;
        // The guard moves to the layer that was empty.
        map.layers[0].objects.push(guard);
        let saved = write_map(MAP, &map).unwrap();
        assert!(saved.contains(r#"nextobjectid="6""#), "{saved}");
        assert!(saved.contains(r#" <tileset firstgid="1" source="../tilesets/tiles.tsx"/>"#));
        assert!(saved.contains("<!-- A comment with <tags> -->"));
        assert_eq!(read_map(&saved).unwrap(), map);
        // Emptying a layer writes it as one tag again.
        map.layers[0].objects.clear();
        let again = write_map(&saved, &map).unwrap();
        assert!(
            again.contains(r#"<objectgroup id="3" name="deco"/>"#),
            "{again}"
        );
        assert_eq!(read_map(&again).unwrap(), map);
    }

    #[test]
    fn single_line_files_stay_single_line() {
        let source = r#"<map nextobjectid="3">
 <objectgroup id="1" name="objects">
  <object id="1" name="start" type="PlayerStart" x="81" y="324"><point/></object>
  <object id="2" gid="204" x="177" y="324" width="24" height="24"><properties><property name="speed" type="int" value="24"/></properties></object>
 </objectgroup>
</map>"#;
        let mut map = read_map(source).unwrap();
        assert!(!map.multiline);
        assert_eq!(write_map(source, &map).unwrap(), source);
        map.layers[0].objects[1].properties[0].value = PropertyData::Int(30);
        let saved = write_map(source, &map).unwrap();
        assert!(saved.contains(r#"  <object id="2" gid="204" x="177" y="324" width="24" height="24"><properties><property name="speed" type="int" value="30"/></properties></object>"#), "{saved}");
    }

    #[test]
    fn the_platformer_maps_round_trip() {
        for name in ["level1", "level2", "level3", "title"] {
            let path = format!(
                "{}/../../games/platformer/assets/maps/{name}.tmx",
                env!("CARGO_MANIFEST_DIR")
            );
            let source = std::fs::read_to_string(path).unwrap();
            let map = read_map(&source).unwrap();
            assert_eq!(write_map(&source, &map).unwrap(), source, "{name}");
        }
    }

    #[test]
    fn broken_files_are_errors() {
        for source in [
            "<map><objectgroup id=\"1\"></map>",
            "<map",
            "<tileset/>",
            "text",
        ] {
            assert!(read_map(source).is_err(), "{source}");
        }
        let map = read_map(MAP).unwrap();
        let other = MAP.replace(r#"<objectgroup id="5""#, r#"<objectgroup id="6""#);
        assert!(write_map(&other, &map).is_err(), "layer 5 is gone");
    }
}
