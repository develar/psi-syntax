//! `PsiModifier` (`java/java-frontback-psi-api/src/com/intellij/psi/PsiModifier.java`): the modifier names, and a set
//! of them.

use crate::tokens::TokenKind;

/// A modifier name of `PsiModifier`, in the order of its constants.
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum Modifier {
    PUBLIC,
    PROTECTED,
    PRIVATE,
    /// No `public`, `protected` or `private` among the explicit and implicit modifiers (`calcModifiers`).
    PACKAGE_LOCAL,
    STATIC,
    ABSTRACT,
    FINAL,
    NATIVE,
    SYNCHRONIZED,
    STRICTFP,
    TRANSIENT,
    VOLATILE,
    DEFAULT,
    OPEN,
    TRANSITIVE,
    SEALED,
    NON_SEALED,
    VALUE,
}

impl Modifier {
    /// Every modifier, in the order of `PsiModifier`'s constants.
    pub const ALL: [Modifier; 18] = [
        Modifier::PUBLIC,
        Modifier::PROTECTED,
        Modifier::PRIVATE,
        Modifier::PACKAGE_LOCAL,
        Modifier::STATIC,
        Modifier::ABSTRACT,
        Modifier::FINAL,
        Modifier::NATIVE,
        Modifier::SYNCHRONIZED,
        Modifier::STRICTFP,
        Modifier::TRANSIENT,
        Modifier::VOLATILE,
        Modifier::DEFAULT,
        Modifier::OPEN,
        Modifier::TRANSITIVE,
        Modifier::SEALED,
        Modifier::NON_SEALED,
        Modifier::VALUE,
    ];

    /// The constant's value (`PsiModifier.PACKAGE_LOCAL` is `"packageLocal"`, `NON_SEALED` `"non-sealed"`).
    pub const fn name(self) -> &'static str {
        match self {
            Modifier::PUBLIC => "public",
            Modifier::PROTECTED => "protected",
            Modifier::PRIVATE => "private",
            Modifier::PACKAGE_LOCAL => "packageLocal",
            Modifier::STATIC => "static",
            Modifier::ABSTRACT => "abstract",
            Modifier::FINAL => "final",
            Modifier::NATIVE => "native",
            Modifier::SYNCHRONIZED => "synchronized",
            Modifier::STRICTFP => "strictfp",
            Modifier::TRANSIENT => "transient",
            Modifier::VOLATILE => "volatile",
            Modifier::DEFAULT => "default",
            Modifier::OPEN => "open",
            Modifier::TRANSITIVE => "transitive",
            Modifier::SEALED => "sealed",
            Modifier::NON_SEALED => "non-sealed",
            Modifier::VALUE => "value",
        }
    }

    /// `PsiModifierListImpl.NAME_TO_KEYWORD_TYPE_MAP` (`PsiModifierListImpl.java:75`): the keyword of a modifier,
    /// `None` for `PACKAGE_LOCAL`.
    pub const fn keyword(self) -> Option<TokenKind> {
        Some(match self {
            Modifier::PUBLIC => TokenKind::PUBLIC_KEYWORD,
            Modifier::PROTECTED => TokenKind::PROTECTED_KEYWORD,
            Modifier::PRIVATE => TokenKind::PRIVATE_KEYWORD,
            Modifier::PACKAGE_LOCAL => return None,
            Modifier::STATIC => TokenKind::STATIC_KEYWORD,
            Modifier::ABSTRACT => TokenKind::ABSTRACT_KEYWORD,
            Modifier::FINAL => TokenKind::FINAL_KEYWORD,
            Modifier::NATIVE => TokenKind::NATIVE_KEYWORD,
            Modifier::SYNCHRONIZED => TokenKind::SYNCHRONIZED_KEYWORD,
            Modifier::STRICTFP => TokenKind::STRICTFP_KEYWORD,
            Modifier::TRANSIENT => TokenKind::TRANSIENT_KEYWORD,
            Modifier::VOLATILE => TokenKind::VOLATILE_KEYWORD,
            Modifier::DEFAULT => TokenKind::DEFAULT_KEYWORD,
            Modifier::OPEN => TokenKind::OPEN_KEYWORD,
            Modifier::TRANSITIVE => TokenKind::TRANSITIVE_KEYWORD,
            Modifier::SEALED => TokenKind::SEALED_KEYWORD,
            Modifier::NON_SEALED => TokenKind::NON_SEALED_KEYWORD,
            Modifier::VALUE => TokenKind::VALUE_KEYWORD,
        })
    }

    /// `PsiModifierListImpl.KEYWORD_TYPE_TO_NAME_MAP`: the modifier a keyword spells.
    pub const fn of_keyword(token: TokenKind) -> Option<Modifier> {
        Some(match token {
            TokenKind::PUBLIC_KEYWORD => Modifier::PUBLIC,
            TokenKind::PROTECTED_KEYWORD => Modifier::PROTECTED,
            TokenKind::PRIVATE_KEYWORD => Modifier::PRIVATE,
            TokenKind::STATIC_KEYWORD => Modifier::STATIC,
            TokenKind::ABSTRACT_KEYWORD => Modifier::ABSTRACT,
            TokenKind::FINAL_KEYWORD => Modifier::FINAL,
            TokenKind::NATIVE_KEYWORD => Modifier::NATIVE,
            TokenKind::SYNCHRONIZED_KEYWORD => Modifier::SYNCHRONIZED,
            TokenKind::STRICTFP_KEYWORD => Modifier::STRICTFP,
            TokenKind::TRANSIENT_KEYWORD => Modifier::TRANSIENT,
            TokenKind::VOLATILE_KEYWORD => Modifier::VOLATILE,
            TokenKind::DEFAULT_KEYWORD => Modifier::DEFAULT,
            TokenKind::OPEN_KEYWORD => Modifier::OPEN,
            TokenKind::TRANSITIVE_KEYWORD => Modifier::TRANSITIVE,
            TokenKind::SEALED_KEYWORD => Modifier::SEALED,
            TokenKind::NON_SEALED_KEYWORD => Modifier::NON_SEALED,
            TokenKind::VALUE_KEYWORD => Modifier::VALUE,
            _ => return None,
        })
    }
}

/// A set of modifiers (what `PsiModifierListImpl`'s `ModifierCache` holds), a bit per `Modifier`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Default, Debug)]
pub struct Modifiers(u32);

impl Modifiers {
    pub const EMPTY: Modifiers = Modifiers(0);

    /// The set of these modifiers.
    pub const fn of(modifiers: &[Modifier]) -> Modifiers {
        let mut bits = 0;
        let mut i = 0;
        while i < modifiers.len() {
            bits |= 1 << modifiers[i] as u32;
            i += 1;
        }
        Modifiers(bits)
    }

    #[inline]
    pub const fn contains(self, modifier: Modifier) -> bool {
        self.0 & (1 << modifier as u32) != 0
    }

    #[inline]
    pub const fn with(self, modifier: Modifier) -> Modifiers {
        Modifiers(self.0 | (1 << modifier as u32))
    }

    #[inline]
    pub const fn union(self, other: Modifiers) -> Modifiers {
        Modifiers(self.0 | other.0)
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The bits, bit `m as u32` for each modifier `m`.
    #[inline]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// The modifiers of the set, in `Modifier::ALL` order.
    pub fn iter(self) -> impl Iterator<Item = Modifier> {
        Modifier::ALL.into_iter().filter(move |&m| self.contains(m))
    }
}
