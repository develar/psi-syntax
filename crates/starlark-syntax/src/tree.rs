//! The Starlark file kinds and the shared parser/tree types.

use std::borrow::Cow;

use crate::Starlark;
use psi_builder::document::{blank_byte_order_mark, has_byte_order_mark};

pub use psi_builder::tree::{LeafIndex, NodeId};

pub type Parser = psi_builder::Parser<Starlark>;
pub type Tree = psi_builder::Tree<Starlark>;

/// IntelliJ's document text: a BOM as three spaces and carriage returns at the same byte offsets.
pub fn document_text(src: &[u8]) -> Cow<'_, [u8]> {
    if !has_byte_order_mark(src) && !src.contains(&b'\r') {
        return Cow::Borrowed(src);
    }
    let mut text = src.to_vec();
    blank_byte_order_mark(&mut text);
    for (index, &byte) in src.iter().enumerate() {
        if byte == b'\r' {
            text[index] = if src.get(index + 1) == Some(&b'\n') { b' ' } else { b'\n' };
        }
    }
    Cow::Owned(text)
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum FileKind {
    Build,
    BuildBazel,
    Workspace,
    WorkspaceBazel,
    WorkspaceBzlmod,
    ModuleBazel,
    RepoBazel,
    #[default]
    Bzl,
    Bazel,
    Star,
    BuildTemplate,
}

impl FileKind {
    /// Whether a top-level call in this file declares a Bazel target.
    pub const fn top_level_calls_declare(self) -> bool {
        matches!(self, FileKind::Build | FileKind::BuildBazel | FileKind::ModuleBazel)
    }

    pub fn from_path(path: &str) -> Option<FileKind> {
        let name = path.rsplit('/').next().unwrap_or(path).rsplit('\\').next().unwrap_or(path);
        Some(match name {
            "BUILD" => FileKind::Build,
            "BUILD.bazel" => FileKind::BuildBazel,
            "WORKSPACE" => FileKind::Workspace,
            "WORKSPACE.bazel" => FileKind::WorkspaceBazel,
            "WORKSPACE.bzlmod" => FileKind::WorkspaceBzlmod,
            "MODULE.bazel" => FileKind::ModuleBazel,
            "REPO.bazel" => FileKind::RepoBazel,
            _ if name.ends_with(".bzl") => FileKind::Bzl,
            _ if name.ends_with(".bazel") => FileKind::Bazel,
            _ if name.ends_with(".star") => FileKind::Star,
            _ if name.ends_with(".BUILD") => FileKind::BuildTemplate,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::FileKind;

    #[test]
    fn recognizes_bazel_names_and_starlark_suffixes() {
        let cases = [
            ("BUILD", FileKind::Build),
            ("BUILD.bazel", FileKind::BuildBazel),
            ("WORKSPACE", FileKind::Workspace),
            ("WORKSPACE.bazel", FileKind::WorkspaceBazel),
            ("WORKSPACE.bzlmod", FileKind::WorkspaceBzlmod),
            ("MODULE.bazel", FileKind::ModuleBazel),
            ("REPO.bazel", FileKind::RepoBazel),
            ("jvm.bzl", FileKind::Bzl),
            ("rule.bazel", FileKind::Bazel),
            ("macros.star", FileKind::Star),
            ("rules.BUILD", FileKind::BuildTemplate),
            ("pkg/BUILD.bazel", FileKind::BuildBazel),
        ];
        for (path, expected) in cases {
            assert_eq!(FileKind::from_path(path), Some(expected), "{path}");
        }
        assert_eq!(FileKind::from_path("BUILD.txt"), None);
    }

    #[test]
    fn only_build_kinds_declare_targets_from_top_level_calls() {
        let cases = [
            (FileKind::Build, true),
            (FileKind::BuildBazel, true),
            (FileKind::ModuleBazel, true),
            (FileKind::Workspace, false),
            (FileKind::WorkspaceBazel, false),
            (FileKind::WorkspaceBzlmod, false),
            (FileKind::RepoBazel, false),
            (FileKind::Bzl, false),
            (FileKind::Bazel, false),
            (FileKind::Star, false),
            (FileKind::BuildTemplate, false),
        ];
        for (kind, expected) in cases {
            assert_eq!(kind.top_level_calls_declare(), expected, "{kind:?}");
        }
    }
}
