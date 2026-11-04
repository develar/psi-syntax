#!/usr/bin/env python3
"""Writes the generated sources of crates/csharp-syntax from Roslyn at the pinned revision (see AGENTS.md), read with
`git show REV:PATH` from a clone (never a moving working tree):

- `src/syntax_kind.rs` from `Syntax/SyntaxKind.cs` and `Syntax/Syntax.xml`: `SyntaxKind` (every kind with Roslyn's
  number), `TokenKind` (the token kinds, `SyntaxFacts.IsAnyToken`, and the unstructured trivia kinds) and `NodeKind`
  (the kinds of Syntax.xml's nodes, structured trivia among them), dense, in Roslyn's numeric order, mapped both ways;
- `src/syntax.rs` from `Syntax/Syntax.xml`: per node type its kinds, slots (field order: token, node, list, separated
  list, optional) and boolean fields, and the green factory functions of `InternalSyntax.SyntaxFactory` (snake_case);
- `src/error_code.rs` from `Errors/ErrorCode.cs`, `Errors/MessageID.cs`, `Errors/XmlParseErrorCode.cs` and
  `CSharpResources.resx`: the error codes the parser and lexer report (every `ErrorCode.X` in `Parser/*.cs`, plus what
  a feature-availability diagnostic reports), their numbers, severities and English message formats; the `MessageID`s
  (`IDS_Feature*` and the others `Parser/*.cs` names) with their messages and `RequiredVersion`; `XmlParseErrorCode`;
- `src/xml_char_type.rs` from `Core/Portable/Xml/XmlCharType.cs`: its literal character property tables (the lexer's
  `IsXmlNameStartChar` and `IsXmlNameChar` read the XML 1.0 4th edition bits);
- `src/syntax_getters/fields.rs` from `Syntax/Syntax.xml`: the getters of every field of every node type and abstract
  class over the tree, the kinds each slot takes (`slot_specs`), and the printer of their `getters` lines; with it
  `tools/csharp-oracle/Fields.g.cs`, the oracle's printer of the same lines from Roslyn's own properties.

    tools/csharp-gen/gen.py [--roslyn DIR] [--rev REV] [--out crates/csharp-syntax/src] [--oracle tools/csharp-oracle]
"""

import argparse
import os
import re
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

PIN = "35d9211b841e7613c1d2f8f5af6d628ace696c4c"
PORTABLE = "src/Compilers/CSharp/Portable/"

RUST_KEYWORDS = {
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn",
    "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "static",
    "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while", "abstract", "become", "box", "do",
    "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
}


def snake(name: str) -> str:
    """`ClassDeclaration` -> `class_declaration`, `XmlCDataSection` -> `xml_c_data_section`."""
    out = []
    for i, c in enumerate(name):
        if c.isupper():
            prev = name[i - 1] if i else ""
            nxt = name[i + 1] if i + 1 < len(name) else ""
            if i and (prev.islower() or prev.isdigit() or (prev.isupper() and nxt.islower())):
                out.append("_")
            out.append(c.lower())
        else:
            out.append(c)
    return "".join(out)


def ident(name: str) -> str:
    return name + "_" if name in RUST_KEYWORDS else name


class Roslyn:
    def __init__(self, repo: str, rev: str):
        self.repo, self.rev = repo, rev

    def read(self, path: str) -> str:
        out = subprocess.run(["git", "-C", self.repo, "show", f"{self.rev}:{path}"], check=True, capture_output=True)
        return out.stdout.decode("utf-8-sig")

    def ls(self, path: str) -> list[str]:
        out = subprocess.run(["git", "-C", self.repo, "ls-tree", "--name-only", f"{self.rev}:{path}"], check=True,
                             capture_output=True)
        return out.stdout.decode().split()


def strip_comments(src: str) -> str:
    src = re.sub(r"/\*.*?\*/", "", src, flags=re.S)
    return re.sub(r"//[^\n]*", "", src)


def enum_members(src: str, enum: str) -> list[tuple[str, str]]:
    """The `NAME = VALUE` members of `enum ENUM { ... }` in declaration order (attributes and comments removed)."""
    src = re.sub(r"^\s*\[[^\n]*\]\s*$", "", src, flags=re.M)  # attributes on their own lines
    src = strip_comments(src)
    start = src.index(f"enum {enum}")
    body = src[src.index("{", start) + 1:]
    body = body[:body.index("}")]
    body = re.sub(r"#\w+[^\n]*", "", body)  # #region
    members = []
    for item in body.split(","):
        item = item.strip()
        if not item:
            continue
        if "=" in item:
            name, value = (s.strip() for s in item.split("=", 1))
        else:
            name, value = item, None
        members.append((name, value))
    return members


# ---------------------------------------------------------------------------------------------------------------------
# Syntax.xml

class Field:
    def __init__(self, e):
        self.name = e.get("Name")
        self.type = e.get("Type")
        self.optional = e.get("Optional") == "true"
        self.kinds = [k.get("Name") for k in e.findall("Kind")]

    @property
    def is_bool(self):
        return self.type == "bool"

    @property
    def element_type(self):
        """The declared type, of a list its element's."""
        t = self.type
        return t[t.index("<") + 1:-1] if "<" in t else t

    @property
    def slot_kind(self):
        t = self.type
        if t == "SyntaxToken":
            return "Token"
        if t.startswith("SeparatedSyntaxList<"):
            return "SeparatedList"
        if t.startswith("SyntaxList<"):
            return "List"
        return "Node"


def flatten_fields(e, make_optional: bool) -> list[Field]:
    """`TreeFlattening.FlattenChildren` of Roslyn's syntax generator: the fields of a node in document order, those
    of a `Choice` (and of a `Sequence` inside one) optional, a list never."""
    fields = []
    for c in e:
        if c.tag == "Field":
            f = Field(c)
            if make_optional and f.slot_kind not in ("List", "SeparatedList"):
                f.optional = True
            fields.append(f)
        elif c.tag == "Choice":
            fields.extend(flatten_fields(c, True))
        elif c.tag == "Sequence":
            fields.extend(flatten_fields(c, make_optional))
    return fields


class NodeType:
    def __init__(self, e):
        self.name = e.get("Name")
        self.base = e.get("Base")
        self.kinds = [k.get("Name") for k in e.findall("Kind")]
        self.fields = flatten_fields(e, False)
        self.slots = [f for f in self.fields if not f.is_bool]
        self.bools = [f for f in self.fields if f.is_bool]

    @property
    def short(self):
        return self.name[:-len("Syntax")] if self.name.endswith("Syntax") else self.name


def node_types(xml: str) -> list[NodeType]:
    root = ET.fromstring(xml)
    return [NodeType(e) for e in root.findall("Node")]


# ---------------------------------------------------------------------------------------------------------------------
# SyntaxKind

def syntax_kinds(src: str) -> list[tuple[str, int]]:
    out = []
    for name, value in enum_members(src, "SyntaxKind"):
        if value == "GreenNode.ListKind":
            out.append((name, 1))
        elif value is not None and value.isdigit():
            out.append((name, int(value)))
        else:
            raise SystemExit(f"SyntaxKind.{name} = {value}: not a number")
    return out


IS_ANY_TOKEN_EXTRA = None  # read from SyntaxKindFacts.cs


def is_any_token_extra(facts: str) -> list[str]:
    m = re.search(r"public static bool IsAnyToken\(SyntaxKind kind\)\s*\{(.*?)\n        \}", facts, re.S)
    body = m.group(1)
    if "kind >= SyntaxKind.TildeToken && kind < SyntaxKind.EndOfLineTrivia" not in body:
        raise SystemExit("IsAnyToken changed: update gen.py")
    return re.findall(r"case SyntaxKind\.(\w+):", body)


class KindClasses:
    """The kinds of `SyntaxKind.cs` in Roslyn's numeric order, classified as `gen_syntax_kind` writes them: `tokens`
    (`IsAnyToken`), `trivia` (unstructured trivia: no node type), `nodes` (the kinds of Syntax.xml's node types),
    `other` (`None`, `List`); `token_kinds` is `TokenKind`'s order (tokens and trivia), `nodes` `NodeKind`'s."""

    def __init__(self, roslyn: Roslyn, types: list[NodeType]):
        kinds = syntax_kinds(roslyn.read(PORTABLE + "Syntax/SyntaxKind.cs"))
        number = dict(kinds)
        if len(number) != len(kinds):
            raise SystemExit("duplicate SyntaxKind names")
        by_value = {}
        for name, value in kinds:
            if value in by_value:
                raise SystemExit(f"SyntaxKind {name} and {by_value[value]} share {value}: ToString() is ambiguous")
            by_value[value] = name
        extra = is_any_token_extra(roslyn.read(PORTABLE + "Syntax/SyntaxKindFacts.cs"))
        node_kinds = set()
        for t in types:
            node_kinds.update(t.kinds)
        lo, hi = number["TildeToken"], number["EndOfLineTrivia"]
        ordered = sorted(kinds, key=lambda k: k[1])
        tokens, trivia, nodes, other = [], [], [], []
        for name, value in ordered:
            if name in ("None", "List"):
                other.append(name)
            elif lo <= value < hi or name in extra:
                if name in node_kinds:
                    raise SystemExit(f"{name} is a token kind and a node kind")
                tokens.append(name)
            elif name in node_kinds:
                nodes.append(name)
            else:
                trivia.append(name)  # unstructured trivia: no node type
        unknown = node_kinds - set(number)
        if unknown:
            raise SystemExit(f"Syntax.xml kinds missing from SyntaxKind: {sorted(unknown)}")
        self.kinds, self.number, self.by_value, self.ordered = kinds, number, by_value, ordered
        self.lo, self.hi = lo, hi
        self.tokens, self.trivia, self.nodes, self.other = tokens, trivia, nodes, other
        self.token_kinds = [n for n, _ in ordered if n in set(tokens) | set(trivia)]


def gen_syntax_kind(roslyn: Roslyn, types: list[NodeType]) -> str:
    classes = KindClasses(roslyn, types)
    kinds, number, by_value, ordered = classes.kinds, classes.number, classes.by_value, classes.ordered
    lo, hi = classes.lo, classes.hi
    tokens, trivia, nodes, other = classes.tokens, classes.trivia, classes.nodes, classes.other
    token_kinds = classes.token_kinds
    o = []
    o.append(f"""//! Roslyn's `SyntaxKind` (`Syntax/SyntaxKind.cs` at {roslyn.rev[:10]}) and the two dense kinds of the tree: `TokenKind`
//! (the {len(tokens)} token kinds of `SyntaxFacts.IsAnyToken` and the {len(trivia)} unstructured trivia kinds, the leaves) and `NodeKind` (the
//! {len(nodes)} kinds of the node types of `Syntax/Syntax.xml`, structured trivia among them), each in Roslyn's numeric order.
//! Generated by `tools/csharp-gen/gen.py`; do not edit by hand.

#![allow(clippy::all)]

/// `Microsoft.CodeAnalysis.CSharp.SyntaxKind`: Roslyn's numbers, ordered by them (`kind >= SyntaxKind::TildeToken`
/// compares as in C#). `name()` is `SyntaxKind.ToString()`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
#[repr(u16)]
pub enum SyntaxKind {{""")
    for name, value in kinds:
        o.append(f"    {'#[default] ' if name == 'None' else ''}{name} = {value},")
    o.append("}\n")
    o.append(f"""/// The token and unstructured trivia kinds: the leaves of the tree (`psi_builder::TokenType`), dense.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(u16)]
pub enum TokenKind {{""")
    for name in token_kinds:
        o.append(f"    {name},")
    o.append("}\n")
    o.append(f"""/// The node kinds (`psi_builder::NodeType`), dense; the tree stores `TokenKind::COUNT` + the index.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(u16)]
pub enum NodeKind {{""")
    for name in nodes:
        o.append(f"    {name},")
    o.append("}\n")
    # tables
    base = lo
    span = max(v for _, v in kinds) - base + 1
    tok_index = {n: i for i, n in enumerate(token_kinds)}
    node_index = {n: i for i, n in enumerate(nodes)}
    o.append(f"const RAW_BASE: u16 = {base};")
    o.append(f"/// `TokenKind` index + 1 (0: none) of `SyntaxKind` raw - `RAW_BASE`.")
    o.append(f"static TOKEN_OF_RAW: [u16; {span}] = [")
    row = []
    for v in range(base, base + span):
        n = by_value.get(v)
        row.append(str(tok_index[n] + 1 if n in tok_index else 0))
    o.extend(wrap_numbers(row))
    o.append("];")
    o.append(f"/// `NodeKind` index + 1 (0: none) of `SyntaxKind` raw - `RAW_BASE`.")
    o.append(f"static NODE_OF_RAW: [u16; {span}] = [")
    row = []
    for v in range(base, base + span):
        n = by_value.get(v)
        row.append(str(node_index[n] + 1 if n in node_index else 0))
    o.extend(wrap_numbers(row))
    o.append("];")
    o.append(f"static TOKEN_SYNTAX_KIND: [SyntaxKind; {len(token_kinds)}] = [")
    o.extend(f"    SyntaxKind::{n}," for n in token_kinds)
    o.append("];")
    o.append(f"static NODE_SYNTAX_KIND: [SyntaxKind; {len(nodes)}] = [")
    o.extend(f"    SyntaxKind::{n}," for n in nodes)
    o.append("];")
    o.append(f"static TOKEN_NAMES: [&str; {len(token_kinds)}] = [")
    o.extend(f'    "{n}",' for n in token_kinds)
    o.append("];")
    o.append(f"static NODE_NAMES: [&str; {len(nodes)}] = [")
    o.extend(f'    "{n}",' for n in nodes)
    o.append("];\n")
    o.append(f"""impl SyntaxKind {{
    /// The kind of Roslyn's number `raw`, `None` for a number that is none.
    pub const fn from_raw(raw: u16) -> Option<SyntaxKind> {{
        Some(match raw {{""")
    for name, value in kinds:
        o.append(f"            {value} => SyntaxKind::{name},")
    o.append(f"""            _ => return None,
        }})
    }}

    /// Roslyn's number (`(ushort)kind`).
    #[inline]
    pub const fn raw(self) -> u16 {{
        self as u16
    }}

    /// `SyntaxKind.ToString()`.
    pub fn name(self) -> &'static str {{
        if let Some(t) = self.token_kind() {{
            return t.debug_name();
        }}
        if let Some(n) = self.node_kind() {{
            return n.debug_name();
        }}
        match self {{""")
    for name in other:
        o.append(f'            SyntaxKind::{name} => "{name}",')
    o.append(f"""            _ => "",
        }}
    }}

    /// The leaf kind of a token or unstructured trivia kind.
    #[inline]
    pub fn token_kind(self) -> Option<TokenKind> {{
        let i = (self as u16).wrapping_sub(RAW_BASE) as usize;
        match TOKEN_OF_RAW.get(i) {{
            Some(&k) if k != 0 => Some(TOKEN_KIND_VALUES[k as usize - 1]),
            _ => None,
        }}
    }}

    /// The node kind of a kind of Syntax.xml's node types.
    #[inline]
    pub fn node_kind(self) -> Option<NodeKind> {{
        let i = (self as u16).wrapping_sub(RAW_BASE) as usize;
        match NODE_OF_RAW.get(i) {{
            Some(&k) if k != 0 => Some(NODE_KIND_VALUES[k as usize - 1]),
            _ => None,
        }}
    }}
}}

static TOKEN_KIND_VALUES: [TokenKind; {len(token_kinds)}] = [""")
    o.extend(f"    TokenKind::{n}," for n in token_kinds)
    o.append("];")
    o.append(f"static NODE_KIND_VALUES: [NodeKind; {len(nodes)}] = [")
    o.extend(f"    NodeKind::{n}," for n in nodes)
    o.append("];\n")
    o.append(f"""impl TokenKind {{
    /// The number of token kinds: where node kinds start in the tree (`psi_builder::TokenType::NODE_BASE`).
    pub const COUNT: usize = {len(token_kinds)};

    #[inline]
    pub fn syntax_kind(self) -> SyntaxKind {{
        TOKEN_SYNTAX_KIND[self as usize]
    }}

    /// `SyntaxKind.ToString()`.
    #[inline]
    pub fn debug_name(self) -> &'static str {{
        TOKEN_NAMES[self as usize]
    }}

    #[inline]
    pub fn from_index(index: usize) -> Option<TokenKind> {{
        TOKEN_KIND_VALUES.get(index).copied()
    }}
}}

impl NodeKind {{
    pub const COUNT: usize = {len(nodes)};

    #[inline]
    pub fn syntax_kind(self) -> SyntaxKind {{
        NODE_SYNTAX_KIND[self as usize]
    }}

    /// `SyntaxKind.ToString()`.
    #[inline]
    pub fn debug_name(self) -> &'static str {{
        NODE_NAMES[self as usize]
    }}

    #[inline]
    pub fn from_index(index: usize) -> Option<NodeKind> {{
        NODE_KIND_VALUES.get(index).copied()
    }}
}}""")
    if len(token_kinds) + len(nodes) >= 4096:
        raise SystemExit("more kinds than the tree's KIND_INDEX holds")
    return "\n".join(o) + "\n"


def wrap_numbers(items: list[str], width: int = 116) -> list[str]:
    lines, line = [], "   "
    for it in items:
        if len(line) + len(it) + 2 > width:
            lines.append(line)
            line = "   "
        line += " " + it + ","
    if line.strip():
        lines.append(line)
    return lines


# ---------------------------------------------------------------------------------------------------------------------
# syntax.rs

DIRECTIVE_BASES = {"DirectiveTriviaSyntax", "BranchingDirectiveTriviaSyntax", "ConditionalDirectiveTriviaSyntax"}


def gen_syntax(roslyn: Roslyn, types: list[NodeType]) -> str:
    o = [f"""//! The node types of Roslyn's `Syntax/Syntax.xml` (at {roslyn.rev[:10]}): per type its kinds, its slots in field order
//! (`Slot`: a token, a node, a list or a separated list, optional or not, its declared type) and its boolean fields
//! (bits of a green node's `extra`), and the green factory function of each, named as `InternalSyntax.SyntaxFactory`'s
//! in snake_case: a type with several kinds takes the kind first. `syntax::types::X` names the node type `X`'s index in
//! `NODE_TYPES` (`X::NODE_TYPE`), its slot indices (`X::IDENTIFIER`) and boolean bits (`X::IS_ACTIVE`).
//! Generated by `tools/csharp-gen/gen.py`; do not edit by hand.

#![allow(clippy::all, clippy::too_many_arguments)]

use crate::green::{{Green, GreenArena}};
use crate::syntax_kind::{{NodeKind, SyntaxKind}};

/// What a slot of a node type holds (`Syntax.xml`'s field type).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SlotKind {{
    /// `SyntaxToken`.
    Token,
    /// A node type.
    Node,
    /// `SyntaxList<T>` (null, one element, or a list).
    List,
    /// `SeparatedSyntaxList<T>`: elements and separators alternating, in one list.
    SeparatedList,
}}

/// A slot of a node type.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Slot {{
    /// The field's name in Syntax.xml.
    pub name: &'static str,
    pub kind: SlotKind,
    /// `Optional="true"`: may be null.
    pub optional: bool,
    /// The field's declared type (`Type=`), of a list its element's: `TypeSyntax`, `NameSyntax`,
    /// `SyntaxToken`, `AttributeListSyntax` of `SyntaxList<AttributeListSyntax>`.
    pub type_name: &'static str,
}}

/// A node type of Syntax.xml.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct NodeTypeInfo {{
    /// The class name (`ClassDeclarationSyntax`).
    pub name: &'static str,
    pub kinds: &'static [SyntaxKind],
    pub slots: &'static [Slot],
    /// The boolean fields, bit `i` of `extra` for the `i`th.
    pub bools: &'static [&'static str],
    /// A directive (`DirectiveTriviaSyntax`): sets `ContainsDirectives`.
    pub is_directive: bool,
    /// A structured trivia (`StructuredTriviaSyntax`: directives, documentation comments, skipped tokens): sets
    /// `ContainsStructuredTrivia`.
    pub is_structured_trivia: bool,
}}
"""]
    by_name = {t.name: t for t in types}
    # directive: base chain reaches DirectiveTriviaSyntax
    root = ET.fromstring(roslyn.read(PORTABLE + "Syntax/Syntax.xml"))
    bases = {e.get("Name"): e.get("Base") for e in root.iter() if e.get("Name") and e.get("Base")}

    def derives(name, base):
        while name:
            if name == base:
                return True
            name = bases.get(name)
        return False

    def is_directive(name):
        return derives(name, "DirectiveTriviaSyntax")

    o.append(f"/// Every node type, in Syntax.xml order.\npub static NODE_TYPES: [NodeTypeInfo; {len(types)}] = [")
    for t in types:
        kinds = ", ".join(f"SyntaxKind::{k}" for k in t.kinds)
        slots = ", ".join(
            f'Slot {{ name: "{f.name}", kind: SlotKind::{f.slot_kind}, optional: {"true" if f.optional else "false"}, '
            f'type_name: "{f.element_type}" }}'
            for f in t.slots)
        bools = ", ".join(f'"{f.name}"' for f in t.bools)
        o.append(f'    NodeTypeInfo {{ name: "{t.name}", kinds: &[{kinds}], slots: &[{slots}], bools: &[{bools}], '
                 f'is_directive: {"true" if is_directive(t.name) else "false"}, '
                 f'is_structured_trivia: {"true" if derives(t.name, "StructuredTriviaSyntax") else "false"} }},')
    o.append("];\n")
    # node kind -> type index
    from_kind = {}
    for i, t in enumerate(types):
        for k in t.kinds:
            if k in from_kind:
                raise SystemExit(f"kind {k} in two node types")
            from_kind[k] = i
    o.append("""/// The node type of a node kind (an index into `NODE_TYPES`).
pub fn node_type(kind: NodeKind) -> &'static NodeTypeInfo {
    &NODE_TYPES[NODE_TYPE_OF_KIND[kind as usize] as usize]
}
""")
    # NodeKind order: same as gen_syntax_kind's `nodes`
    kinds = syntax_kinds(roslyn.read(PORTABLE + "Syntax/SyntaxKind.cs"))
    node_order = [n for n, _ in sorted(kinds, key=lambda k: k[1]) if n in from_kind]
    o.append(f"static NODE_TYPE_OF_KIND: [u16; {len(node_order)}] = [")
    o.extend(wrap_numbers([str(from_kind[n]) for n in node_order]))
    o.append("];\n")
    # slot index constants
    o.append("/// The slot indices and boolean bits of each node type, by its Syntax.xml name.")
    o.append("pub mod types {")
    for i, t in enumerate(types):
        o.append(f"    /// `{t.name}`: kinds {', '.join(t.kinds)}.")
        o.append(f"    pub struct {t.name};")
        o.append(f"    impl {t.name} {{")
        o.append(f"        /// Its index in `NODE_TYPES`.")
        o.append(f"        pub const NODE_TYPE: usize = {i};")
        for s, f in enumerate(t.slots):
            o.append(f"        pub const {snake(f.name).upper()}: usize = {s};")
        for b, f in enumerate(t.bools):
            o.append(f"        pub const {snake(f.name).upper()}: u32 = 1 << {b};")
        o.append("    }")
    o.append("}\n")
    # factories
    o.append("impl GreenArena {")
    for t in types:
        params = []
        if len(t.kinds) > 1:
            params.append("kind: SyntaxKind")
        for f in t.fields:
            params.append(f"{ident(snake(f.name))}: {'bool' if f.is_bool else 'Green'}")
        slot_args = ", ".join(ident(snake(f.name)) for f in t.slots)
        extra = " | ".join(f"(({ident(snake(f.name))} as u32) << {b})" for b, f in enumerate(t.bools)) or "0"
        if len(t.bools) == 1:
            extra = f"{ident(snake(t.bools[0].name))} as u32"
        kind = "kind" if len(t.kinds) > 1 else f"SyntaxKind::{t.kinds[0]}"
        doc_kinds = ", ".join(t.kinds)
        o.append(f"    /// `SyntaxFactory.{t.short}`: a `{t.name}` ({doc_kinds}).")
        o.append(f"    pub fn {ident(snake(t.short))}(&mut self, {', '.join(params)}) -> Green {{")
        o.append(f"        self.create_node({kind}, &[{slot_args}], {extra})")
        o.append("    }")
    o.append("}")
    return "\n".join(o) + "\n"


# ---------------------------------------------------------------------------------------------------------------------
# syntax_getters/fields.rs and tools/csharp-oracle/Fields.g.cs

# Token fields whose Syntax.xml names no kind, where an earlier token list must know where it stops: a record's
# keyword is `record` (LanguageParser.cs, `ParseTypeDeclaration`: `case SyntaxKind.RecordKeyword`, the keyword
# converted by `ConvertToKeyword`).
TOKEN_KINDS = {("RecordDeclarationSyntax", "Keyword"): ["RecordKeyword"]}

# Abstract properties a concrete type implements by another of its fields (`NameColonSyntax.cs:11`,
# `public override ExpressionSyntax Expression => Name;`). A concrete type without the field and without an entry
# here answers none, as Roslyn's overrides do (`ExtensionBlockDeclarationSyntax.cs:9` and `:14`: `Identifier =>
# default`, `BaseList => null`).
ABSTRACT_ALIASES = {("NameColonSyntax", "Expression"): "Name"}

# The fields `syntax_getters` also answers on a node of any type that has one (`modifiers_of`, ...).
ANY_FIELDS = ["AttributeLists", "Modifiers", "Identifier", "TypeParameterList", "ParameterList", "ConstraintClauses",
              "BaseList", "Members", "Body", "ExpressionBody", "Type", "ReturnType", "ExplicitInterfaceSpecifier",
              "AccessorList", "SemicolonToken"]


class Hierarchy:
    """Syntax.xml's classes: `Node`s (concrete, `types`), `AbstractNode`s and `PredefinedNode`s by their `Base`."""

    def __init__(self, xml: str, types: list[NodeType]):
        root = ET.fromstring(xml)
        self.bases = {e.get("Name"): e.get("Base") for e in root if e.get("Name")}
        self.abstract = [(e.get("Name"), [f for f in flatten_fields(e, False) if not f.is_bool])
                         for e in root.findall("AbstractNode")]
        self.types = types
        self.structured = {t.name for t in types if self.derives(t.name, "StructuredTriviaSyntax")}

    def derives(self, name: str, base: str) -> bool:
        while name:
            if name == base:
                return True
            name = self.bases.get(name)
        return False

    def concrete(self, base: str) -> list[NodeType]:
        """The node types deriving from `base` (itself when concrete) but structured trivia: no field holds one, and a
        getter skips them as trivia."""
        return [t for t in self.types if t.name not in self.structured and self.derives(t.name, base)]

    def all_concrete(self, base: str) -> list[NodeType]:
        return [t for t in self.types if self.derives(t.name, base)]

    def abstract_fields(self, name: str) -> list[Field]:
        """The fields of the abstract class `name` and of its abstract bases, the bases' first."""
        chain = []
        while name:
            chain.append(name)
            name = self.bases.get(name)
        own = dict(self.abstract)
        return [f for a in reversed(chain) for f in own.get(a, [])]


def list_element(t: str) -> str:
    return t[t.index("<") + 1:-1]


def slot_specs(h: Hierarchy, classes: KindClasses, t: NodeType) -> list[tuple[set, set, bool]]:
    """Per slot of `t` the kinds a child in it may have, the kinds Syntax.xml declares for it, and whether the slot takes
    several (a list): a token slot its
    Syntax.xml kinds (any token without them), a node slot its kinds or those of the node types deriving from its
    type (a slot that is never null takes any token or any node where it stands; its own kinds only say where the
    lists and optional slots before it stop), a list those of its element type, a token list (`SyntaxList<SyntaxToken>`, modifiers) every token that no
    slot after it up to its first required one takes, a separated list its elements and any token as a separator
    (Roslyn's recovery makes a `;` one, `enum C { A ; }`), which `slots.rs` gives to a later slot that takes it only
    when the next child fits after that slot. A child goes to the first slot from the current one that takes it, so an
    earlier slot wins over a later one; two node slots that both take a kind where the first may be skipped are an
    ambiguity this assignment cannot tell apart, and fail the generator."""
    tokens = set(classes.tokens)
    all_nodes = {k for c in h.concrete("CSharpSyntaxNode") for k in c.kinds}
    n = len(t.slots)
    accept = [set()] * n
    declared = [set()] * n
    for i in range(n - 1, -1, -1):
        f = t.slots[i]
        follow = set()
        for j in range(i + 1, n):
            follow |= declared[j]
            if not t.slots[j].optional and t.slots[j].slot_kind in ("Token", "Node"):
                break
        nodes = set()
        if f.slot_kind == "Token":
            a = set(TOKEN_KINDS.get((t.name, f.name), f.kinds) or tokens)
        elif f.slot_kind == "Node":
            nodes = set(f.kinds) if f.kinds else {k for c in h.concrete(f.type) for k in c.kinds}
            a = nodes
        else:
            e = list_element(f.type)
            if e == "SyntaxToken":
                a = tokens - follow
            else:
                nodes = {k for c in h.concrete(e) for k in c.kinds}
                a = nodes | (tokens if f.slot_kind == "SeparatedList" else set())
        if (f.optional or f.slot_kind in ("List", "SeparatedList")) and nodes & follow:
            raise SystemExit(f"{t.name}.{f.name}: a later slot takes its node kinds {sorted(nodes & follow)}")
        declared[i] = a
        # a slot that is never null takes the child where it stands, whatever its kind: Roslyn's recovery puts tokens
        # of other kinds there (`event A.this` names the event by its `this`), and its declared kinds are only where
        # the slots before it stop
        accept[i] = (tokens if f.slot_kind == "Token" else all_nodes) if is_required(f) else a
    return [(accept[i], declared[i], t.slots[i].slot_kind in ("List", "SeparatedList")) for i in range(n)]


def is_required(f: Field) -> bool:
    return not f.optional and f.slot_kind in ("Token", "Node")


def gen_syntax_getters(roslyn: Roslyn, types: list[NodeType]) -> tuple[str, str]:
    """`crates/csharp-syntax/src/syntax_getters/fields.rs` and `tools/csharp-oracle/Fields.g.cs`."""
    h = Hierarchy(roslyn.read(PORTABLE + "Syntax/Syntax.xml"), types)
    classes = KindClasses(roslyn, types)
    index = {k: i for i, k in enumerate(classes.token_kinds)}
    index.update({k: len(classes.token_kinds) + i for i, k in enumerate(classes.nodes)})
    words = (len(index) + 63) // 64
    sets = {}

    def kind_set(kinds) -> str:
        bits = [0] * words
        for k in kinds:
            bits[index[k] // 64] |= 1 << (index[k] % 64)
        key = tuple(bits)
        if key not in sets:
            sets[key] = f"S{len(sets)}"
        return sets[key]

    type_index = {t.name: i for i, t in enumerate(types)}
    trivia = kind_set(set(classes.trivia) | {k for t in types if t.name in h.structured for k in t.kinds})
    spec_rows = []
    for t in types:
        cells = ", ".join(
            f"SlotSpec {{ accept: &{kind_set(a)}, declared: &{kind_set(d)}, many: {str(many).lower()}, separated: "
            f"{str(f.slot_kind == 'SeparatedList').lower()}, required: {str(is_required(f)).lower()} }}"
            for (a, d, many), f in zip(slot_specs(h, classes, t), t.slots))
        spec_rows.append(f"    &[{cells}],")

    def one(f: Field) -> bool:
        return f.slot_kind in ("Token", "Node")

    def ret(f: Field) -> str:
        return "Option<NodeId>" if one(f) else "Elements<'_>"

    def getter(f: Field) -> str:
        """`read_field` of a token or node, `read_tokens` of a token list, `read_nodes` of a list or separated list of
        nodes (`slots.rs`)."""
        if one(f):
            return "read_field"
        return "read_tokens" if list_element(f.type) == "SyntaxToken" else "read_nodes"

    def field_doc(owner: str, f: Field) -> str:
        what = {"Token": "a token", "Node": "a node", "List": "a list",
                "SeparatedList": "a separated list, its elements"}[f.slot_kind]
        opt = ", optional" if f.optional and one(f) else ""
        kinds = f" ({', '.join(f.kinds)})" if f.kinds and len(f.kinds) <= 6 else ""
        return f"`{owner}.{f.name}`: `{f.type}`{kinds}, {what}{opt}."

    mods, printers = [], []
    for i, t in enumerate(types):
        name = ident(snake(t.short))
        body = [f"/// `{t.name}` ({', '.join(t.kinds)}): its fields in Syntax.xml order.",
                f"pub mod {name} {{", "    use super::*;", "", f"    const TYPE: u8 = {i};"]
        lines = []
        for s, f in enumerate(t.slots):
            fn = ident(snake(f.name))
            body.append("")
            body.append(f"    /// {field_doc(t.name, f)}")
            body.append(f"    pub fn {fn}(tree: &Tree, id: NodeId) -> {ret(f)} {{")
            body.append(f"        {getter(f)}(tree, id, TYPE, {s})")
            body.append("    }")
            lines.append(f"            out.{'one' if one(f) else 'list'}(\"{f.name}\", {name}::{fn}(tree, id));")
        body.append("}")
        mods.append("\n".join(body))
        printers.append(f"        {i} => {{\n" + "\n".join(lines) + "\n        }")
    abstract_mods, abstract_printers, is_fns = [], [], []
    for a, own in h.abstract:
        members = h.all_concrete(a)
        kinds = [k for c in members for k in c.kinds]
        fn_is = f"is_{snake(a)}"
        is_fns.append(f"/// A node kind of a type deriving from `{a}` in Syntax.xml ({len(members)} types).\n"
                      f"pub fn {fn_is}(kind: NodeKind) -> bool {{\n"
                      f"    {kind_set(kinds)}.contains(node_index(kind))\n}}")
        fields = h.abstract_fields(a)
        if not fields:
            continue
        name = ident(snake(a[:-len("Syntax")]))
        body = [f"/// `{a}`: the fields it and its abstract bases declare, on a node of a type deriving from it "
                f"({', '.join(c.name for c in members)}).", f"pub mod {name} {{", "    use super::*;"]
        for f in fields:
            table = []
            for c in members:
                target = ABSTRACT_ALIASES.get((c.name, f.name), f.name)
                for s, g in enumerate(c.slots):
                    if g.name == target:
                        if getter(g) != getter(f):
                            raise SystemExit(f"{a}.{f.name} is a {f.slot_kind}, {c.name}.{g.name} a {g.slot_kind}")
                        table.append(f"({type_index[c.name]}, {s})")
            body.append("")
            body.append(f"    /// {field_doc(a, f)} {'None' if one(f) else 'Empty'} on a type without it.")
            body.append(f"    pub fn {ident(snake(f.name))}(tree: &Tree, id: NodeId) -> {ret(f)} {{")
            body.append(f"        {getter(f)}_in(tree, id, &[{', '.join(table)}])")
            body.append("    }")
        body.append("}")
        abstract_mods.append("\n".join(body))
        if own:
            lines = [f"    if {fn_is}(kind) {{"]
            for f in own:
                lines.append(f"        out.{'one' if one(f) else 'list'}(\"{a}.{f.name}\", "
                             f"{name}::{ident(snake(f.name))}(tree, id));")
            lines.append("    }")
            abstract_printers.append("\n".join(lines))
    any_fns = []
    for fname in ANY_FIELDS:
        table, shapes, proto = [], set(), None
        for i, t in enumerate(types):
            for s, f in enumerate(t.slots):
                if f.name == fname:
                    table.append(f"({i}, {s})")
                    shapes.add(getter(f))
                    proto = f
        if len(shapes) != 1:
            raise SystemExit(f"the field {fname} is of several shapes: {shapes}")
        any_fns.append(f"/// `{fname}` of a node of any type that has the field ({len(table)} types), "
                       f"{'None' if one(proto) else 'empty'} on the others.\n"
                       f"pub fn {snake(fname)}_of(tree: &Tree, id: NodeId) -> {ret(proto)} {{\n"
                       f"    {getter(proto)}_in(tree, id, &[{', '.join(table)}])\n}}")
    type_slots = []
    for t in types:
        bits = sum(1 << i for i, f in enumerate(t.slots) if f.element_type != "SyntaxToken" and h.derives(f.element_type, "TypeSyntax"))
        type_slots.append(f"0x{bits:x}")
    if max(len(t.slots) for t in types) > 16:
        raise SystemExit("a node type of more slots than TYPE_SLOTS' u16 holds")
    node_type = [type_index[next(t.name for t in types if k in t.kinds)] for k in classes.nodes]
    if len(types) > 255:
        raise SystemExit("more node types than a u8 holds")
    set_defs = [f"static {name}: KindSet = KindSet([{', '.join(f'0x{b:x}' for b in bits)}]);"
                for bits, name in sets.items()]
    nl = "\n"
    rust = f"""//! The fields of Roslyn's node types (`Syntax/Syntax.xml` at {roslyn.rev[:10]}) as getters over the tree: a module per
//! node type (`class_declaration::identifier` is `ClassDeclarationSyntax.Identifier`) and per abstract class with
//! fields (`type_declaration::members` on a class, a struct, a record, an interface or an extension block;
//! `member_declaration::modifiers` on any member), a function per field in snake_case; `is_<abstract class>`
//! (`is_type_syntax`) per abstract class; `<field>_of` (`modifiers_of`) for a few fields on a node of any type that has
//! one. A token or node field answers `Option<NodeId>`: None where Roslyn's slot is null and on a node of another type,
//! `Some` for a missing token (`IsMissing`, an empty leaf); a list answers its children in order (`Elements`), a
//! separated list its elements without the separators. Each reads the node's children in slot order up to its field
//! (`super::slots`). Generated by `tools/csharp-gen/gen.py`; do not edit by hand.

use super::getters::Lines;
use super::slots::{{
    node_index, read_field, read_field_in, read_nodes, read_nodes_in, read_tokens, read_tokens_in, Elements, KindSet,
    SlotSpec,
}};
use crate::syntax_kind::NodeKind;
use crate::tree::{{NodeId, Tree}};

/// The words of a `KindSet`: a bit per `TokenKind` and per `NodeKind` (`TokenKind::COUNT` + its index).
pub(super) const WORDS: usize = {words};

{nl.join(set_defs)}

/// What a getter skips: the unstructured trivia leaves and the roots of structured trivia.
pub(super) static TRIVIA: &KindSet = &{trivia};

/// Per node type (`syntax::NODE_TYPES`' order) its slots: the kinds each takes and the kinds Syntax.xml declares for it,
/// whether it takes several, whether it is a separated list, whether it is a token or node that is never null.
pub(super) static SPECS: [&[SlotSpec]; {len(types)}] = [
{nl.join(spec_rows)}
];

/// Per node type the slots whose declared type (of a list, its element's) derives from `TypeSyntax`, bit `i` for slot `i`.
pub(super) static TYPE_SLOTS: [u16; {len(types)}] = [
{nl.join(wrap_numbers(type_slots))}
];

/// The node type (an index into `SPECS` and `syntax::NODE_TYPES`) of each `NodeKind`.
pub(super) static TYPE_OF_NODE: [u8; {len(node_type)}] = [
{nl.join(wrap_numbers([str(x) for x in node_type]))}
];

{(nl + nl).join(is_fns)}

{(nl + nl).join(any_fns)}

{(nl + nl).join(mods)}

{(nl + nl).join(abstract_mods)}

/// The lines of `getters` of a node's fields: those of its type, then those of each abstract class it derives from,
/// in Syntax.xml order, prefixed with the class (`TypeDeclarationSyntax.Keyword`).
pub(super) fn print_fields(tree: &Tree, id: NodeId, kind: NodeKind, out: &mut Lines) {{
    match TYPE_OF_NODE[kind as usize] {{
{nl.join(printers)}
        _ => {{}}
    }}
{nl.join(abstract_printers)}
}}
"""

    def cs_getter(f: Field) -> str:
        if one(f):
            return f.slot_kind
        if f.slot_kind == "SeparatedList":
            return "Separated"
        return "Tokens" if list_element(f.type) == "SyntaxToken" else "List"

    cs = [f"""// The fields of Roslyn's node types (Syntax/Syntax.xml at {roslyn.rev[:10]}) as the getters mode prints them, in the
// order of crates/csharp-syntax/src/syntax_getters/fields.rs's print_fields: a node's own fields, then those of each
// abstract class it derives from. Generated by tools/csharp-gen/gen.py; do not edit by hand.
// The node types of the preview features (union declarations, `with` elements, unsafe expressions, labels of `break`
// and `continue`) are marked experimental; the oracle reads them all the same.
#pragma warning disable RSEXPERIMENTAL006
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace CSharpOracle;

static class Fields
{{
    public static void Print(SyntaxNode node, GetterLines o)
    {{
        switch (node)
        {{"""]
    for t in types:
        cs.append(f"            case {t.name} n:")
        for f in t.slots:
            cs.append(f"                o.{cs_getter(f)}(\"{f.name}\", n.{f.name});")
        cs.append("                break;")
    cs.append("        }")
    for i, (a, own) in enumerate(h.abstract):
        if not own:
            continue
        cs.append(f"        if (node is {a} a{i})")
        cs.append("        {")
        for f in own:
            cs.append(f"            o.{cs_getter(f)}(\"{a}.{f.name}\", a{i}.{f.name});")
        cs.append("        }")
    cs.append("    }\n}\n")
    return rust, "\n".join(cs)


# ---------------------------------------------------------------------------------------------------------------------
# error_code.rs

def parser_sources(roslyn: Roslyn) -> list[str]:
    names = roslyn.ls(PORTABLE + "Parser/")
    return [roslyn.read(PORTABLE + "Parser/" + n) for n in names if n.endswith(".cs")]


def gen_error_code(roslyn: Roslyn) -> str:
    resx = ET.fromstring(roslyn.read(PORTABLE + "CSharpResources.resx").encode("utf-8"))
    messages = {}
    for e in resx.findall("data"):
        v = e.find("value")
        messages[e.get("name")] = v.text if v is not None and v.text is not None else ""
    codes_src = roslyn.read(PORTABLE + "Errors/ErrorCode.cs")
    numbers = {}
    for name, value in enum_members(codes_src, "ErrorCode"):
        if value is not None and value.isdigit():
            numbers[name] = int(value)
    sources = parser_sources(roslyn)
    used = set()
    for s in sources:
        used.update(re.findall(r"(?<![A-Za-z])ErrorCode\.(\w+)", strip_comments(s)))
    # GetDisabledFeatureDiagnosticInfo, LanguageVersion.GetErrorCode, CheckFeatureAvailability's forceWarning
    lv = roslyn.read(PORTABLE + "LanguageVersion.cs")
    used.update(re.findall(r"ErrorCode\.(ERR_FeatureNotAvailableInVersion\w+)", lv))
    used.update({"ERR_FeatureInPreview", "ERR_FeatureIsExperimental", "WRN_ErrorOverride", "WRN_XMLParseError"})
    used.discard("Void")
    used.discard("Unknown")
    missing = sorted(u for u in used if u not in numbers)
    if missing:
        raise SystemExit(f"ErrorCode without a number: {missing}")
    codes = sorted(used, key=lambda n: numbers[n])
    # MessageID
    mid_src = roslyn.read(PORTABLE + "Errors/MessageID.cs")
    mids = []
    for name, value in enum_members(mid_src, "MessageID"):
        m = re.fullmatch(r"MessageBase \+ (\d+)", value or "")
        if m:
            mids.append((name, 1200 + int(m.group(1))))
    used_mids = set()
    for s in sources:
        used_mids.update(re.findall(r"MessageID\.(\w+)", strip_comments(s)))
    mid_names = [n for n, _ in mids if n.startswith("IDS_Feature") or n in used_mids]
    known = {n for n, _ in mids}
    missing = sorted(u for u in used_mids if u not in known)
    if missing:
        raise SystemExit(f"MessageID without a number: {missing}")
    # RequiredVersion
    body = mid_src[mid_src.index("internal static LanguageVersion RequiredVersion(this MessageID feature)"):]
    body = body[:body.index("default:")]
    required = {}
    pending = []
    for line in strip_comments(body).splitlines():
        m = re.search(r"case MessageID\.(\w+):", line)
        if m:
            pending.append(m.group(1))
        m = re.search(r"return LanguageVersion\.(\w+);", line)
        if m:
            for p in pending:
                required[p] = m.group(1)
            pending = []
    xml_src = roslyn.read(PORTABLE + "Errors/XmlParseErrorCode.cs")
    xml_codes = [n for n, _ in enum_members(xml_src, "XmlParseErrorCode")]

    def rust_str(s: str) -> str:
        return '"' + s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\r", "\\r") + '"'

    def severity(name):
        if name.startswith("WRN_"):
            return "Warning"
        if name.startswith("INF_"):
            return "Info"
        if name.startswith("HDN_"):
            return "Hidden"
        return "Error"

    for n in codes + xml_codes:
        if n not in messages:
            raise SystemExit(f"no message for {n} in CSharpResources.resx")
    # A MessageID without a resource localizes to null (`ResourceManager.GetString`), which String.Format prints as "".
    unnamed = [n for n in mid_names if n not in messages]
    for n in unnamed:
        messages[n] = ""
    o = [f"""//! The diagnostics of Roslyn's parser and lexer (at {roslyn.rev[:10]}): `ErrorCode` (every code `Parser/*.cs` names, and
//! what a feature-availability diagnostic reports), its number (`CSnnnn`), severity (by its prefix, as
//! `ErrorFacts.GetSeverity`) and English message format (`CSharpResources.resx`); `MessageID` (the `IDS_Feature*` and the
//! others `Parser/*.cs` names) with its message and `RequiredVersion`; `XmlParseErrorCode` with its message.
//! Generated by `tools/csharp-gen/gen.py`; do not edit by hand.

#![allow(non_camel_case_types, clippy::all)]

use crate::language_version::LanguageVersion;

/// `DiagnosticSeverity`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Severity {{
    Hidden,
    Info,
    Warning,
    Error,
}}

/// `Microsoft.CodeAnalysis.CSharp.ErrorCode`: the codes the parser and lexer report.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[repr(u16)]
pub enum ErrorCode {{"""]
    for n in codes:
        o.append(f"    {n},")
    o.append("}\n")
    o.append(f"static ERROR_NUMBERS: [u16; {len(codes)}] = [")
    o.extend(wrap_numbers([str(numbers[n]) for n in codes]))
    o.append("];")
    o.append(f"static ERROR_NAMES: [&str; {len(codes)}] = [")
    o.extend(f'    "{n}",' for n in codes)
    o.append("];")
    o.append(f"static ERROR_MESSAGES: [&str; {len(codes)}] = [")
    o.extend(f"    {rust_str(messages[n])}," for n in codes)
    o.append("];")
    o.append(f"static ERROR_SEVERITIES: [Severity; {len(codes)}] = [")
    o.extend(f"    Severity::{severity(n)}," for n in codes)
    o.append("];\n")
    o.append("""impl ErrorCode {
    /// The number (`(int)code`): the diagnostic's id is `CS` and it in four digits.
    #[inline]
    pub fn number(self) -> u16 {
        ERROR_NUMBERS[self as usize]
    }

    /// The name (`ERR_SyntaxError`).
    pub fn name(self) -> &'static str {
        ERROR_NAMES[self as usize]
    }

    /// The English message format (`{0}` placeholders), as `ResourceManager.GetString(code.ToString())` returns it.
    pub fn message_format(self) -> &'static str {
        ERROR_MESSAGES[self as usize]
    }

    /// `ErrorFacts.GetSeverity`.
    pub fn severity(self) -> Severity {
        ERROR_SEVERITIES[self as usize]
    }
}
""")
    o.append("/// `Microsoft.CodeAnalysis.CSharp.MessageID`: the features (`IDS_Feature*`) and the other ids the parser names.")
    o.append("#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]\n#[repr(u16)]\npub enum MessageID {")
    for n in mid_names:
        o.append(f"    {n},")
    o.append("}\n")
    o.append(f"static MESSAGE_ID_MESSAGES: [&str; {len(mid_names)}] = [")
    o.extend(f"    {rust_str(messages[n])}," for n in mid_names)
    o.append("];\n")
    o.append("impl MessageID {")
    o.append("    /// `MessageID.Localize()`'s English text (`ErrorFacts.GetMessage(id)`): the feature's name in a message; \"\" for")
    o.append(f"    /// the ids without a resource ({', '.join(unnamed) or 'none'}), which format as null.")
    o.append("    pub fn message(self) -> &'static str {\n        MESSAGE_ID_MESSAGES[self as usize]\n    }\n")
    o.append("    /// `MessageIDExtensions.RequiredVersion`: the version a feature needs (`None` where Roslyn throws).")
    o.append("    pub fn required_version(self) -> Option<LanguageVersion> {\n        Some(match self {")
    for n in mid_names:
        if n in required:
            v = required[n]
            o.append(f"            MessageID::{n} => LanguageVersion::{v},")
    if any(n not in required for n in mid_names):
        o.append("            _ => return None,")
    o.append("        })\n    }\n}\n")
    o.append("/// `XmlParseErrorCode`: the detail of a `WRN_XMLParseError` (`XmlSyntaxDiagnosticInfo`).")
    o.append("#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]\n#[repr(u8)]\npub enum XmlParseErrorCode {")
    for n in xml_codes:
        o.append(f"    {n},")
    o.append("}\n")
    o.append(f"static XML_MESSAGES: [&str; {len(xml_codes)}] = [")
    o.extend(f"    {rust_str(messages[n])}," for n in xml_codes)
    o.append("];\n")
    o.append("""impl XmlParseErrorCode {
    /// The English message format.
    pub fn message_format(self) -> &'static str {
        XML_MESSAGES[self as usize]
    }
}""")
    return "\n".join(o) + "\n"


# ---------------------------------------------------------------------------------------------------------------------
# xml_char_type.rs

def gen_xml_char_type(roslyn: Roslyn) -> str:
    src = roslyn.read("src/Compilers/Core/Portable/Xml/XmlCharType.cs")
    assert "#define XMLCHARTYPE_USE_LITERAL_TABLE" in src

    def table(name: str) -> list[int]:
        m = re.search(r"private static readonly byte\[\] " + name + r"\s*=\s*\{(.*?)\};", src, re.S)
        return [int(x, 16) for x in re.findall(r"0x[0-9a-fA-F]+", m.group(1))]

    def flag(name: str) -> int:
        return int(re.search(r"internal const int " + name + r" = (\d+);", src).group(1))

    index, props = table("s_charPropertiesIndex"), table("s_charProperties")
    assert len(index) == 256 and len(props) % 256 == 0 and max(index) * 256 < len(props)
    o = [
        "//! `XmlCharType` (Roslyn's `Core/Portable/Xml/XmlCharType.cs`, `XMLCHARTYPE_USE_LITERAL_TABLE`): the character",
        "//! properties of a UTF-16 code unit as its literal tables have them, of which the lexer asks the XML 1.0 4th edition",
        "//! name classes. Generated by `tools/csharp-gen/gen.py`; do not edit by hand.",
        "",
        f"const F_LETTER: u8 = {flag('fLetter')};",
        f"const F_NC_NAME_XML4E: u8 = {flag('fNCNameXml4e')};",
        "",
        "/// `s_charPropertiesIndex`: the 256-entry table of each high byte in `CHAR_PROPERTIES`.",
        "#[rustfmt::skip]",
        f"static CHAR_PROPERTIES_INDEX: [u8; {len(index)}] = [",
        *wrap_numbers([f"0x{x:02x}" for x in index]),
        "];",
        "",
        "/// `s_charProperties`.",
        "#[rustfmt::skip]",
        f"static CHAR_PROPERTIES: [u8; {len(props)}] = [",
        *wrap_numbers([f"0x{x:02x}" for x in props]),
        "];",
        "",
        "/// `GetCharProperties(i)`.",
        "#[inline]",
        "fn get_char_properties(i: u16) -> u8 {",
        "    // The index entry, table, identifies the start of the appropriate 256-entry table within s_charProperties",
        "    let table = CHAR_PROPERTIES_INDEX[(i >> 8) as usize] as usize;",
        "    CHAR_PROPERTIES[(table << 8) + (i & 0xff) as usize]",
        "}",
        "",
        "/// `IsLetter(ch)` (XML 1.0 Fourth Edition).",
        "pub(crate) fn is_letter(ch: u16) -> bool {",
        "    get_char_properties(ch) & F_LETTER != 0",
        "}",
        "",
        "/// `IsNCNameCharXml4e(ch)`.",
        "pub(crate) fn is_nc_name_char_xml4e(ch: u16) -> bool {",
        "    get_char_properties(ch) & F_NC_NAME_XML4E != 0",
        "}",
        "",
        "/// `IsStartNCNameCharXml4e(ch)`.",
        "pub(crate) fn is_start_nc_name_char_xml4e(ch: u16) -> bool {",
        "    is_letter(ch) || ch == b'_' as u16",
        "}",
    ]
    return "\n".join(o) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--roslyn", default=os.path.expanduser("~/.cache/psi-syntax/roslyn"))
    ap.add_argument("--rev", default=PIN)
    ap.add_argument("--out", default=str(Path(__file__).resolve().parents[2] / "crates/csharp-syntax/src"))
    ap.add_argument("--oracle", default=str(Path(__file__).resolve().parents[1] / "csharp-oracle"))
    args = ap.parse_args()
    roslyn = Roslyn(args.roslyn, args.rev)
    types = node_types(roslyn.read(PORTABLE + "Syntax/Syntax.xml"))
    out = Path(args.out)
    files = [out / "syntax_kind.rs", out / "syntax.rs", out / "error_code.rs", out / "xml_char_type.rs"]
    files[0].write_text(gen_syntax_kind(roslyn, types))
    files[1].write_text(gen_syntax(roslyn, types))
    files[2].write_text(gen_error_code(roslyn))
    files[3].write_text(gen_xml_char_type(roslyn))
    fields, oracle_fields = gen_syntax_getters(roslyn, types)
    files.append(out / "syntax_getters" / "fields.rs")
    files[4].write_text(fields)
    (Path(args.oracle) / "Fields.g.cs").write_text(oracle_fields)
    # the repository's rustfmt.toml (max_width 120) over the generated files
    config = Path(__file__).resolve().parents[2] / "rustfmt.toml"
    subprocess.run(["rustfmt", "--edition", "2024", "--config-path", str(config), *map(str, files)], check=True)


if __name__ == "__main__":
    sys.exit(main())
