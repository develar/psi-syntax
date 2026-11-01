//! `com.intellij.pom.java.LanguageLevel` and the part of `JavaFeature` the lexers, binders and parsers ask
//! (`isSufficient`).

/// `LanguageLevel`, in declaration order (the order `isAtLeast` compares).
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum LanguageLevel {
    JDK_1_3,
    JDK_1_4,
    JDK_1_5,
    JDK_1_6,
    JDK_1_7,
    JDK_1_8,
    JDK_1_9,
    JDK_10,
    JDK_11,
    JDK_12,
    JDK_13,
    JDK_14,
    JDK_15,
    JDK_16,
    JDK_17,
    JDK_17_PREVIEW,
    JDK_18,
    JDK_18_PREVIEW,
    JDK_19,
    JDK_19_PREVIEW,
    JDK_20,
    JDK_20_PREVIEW,
    JDK_21,
    JDK_21_PREVIEW,
    JDK_22,
    JDK_22_PREVIEW,
    JDK_23,
    JDK_23_PREVIEW,
    JDK_24,
    JDK_24_PREVIEW,
    JDK_25,
    JDK_25_PREVIEW,
    JDK_26,
    JDK_26_PREVIEW,
    JDK_27,
    JDK_27_PREVIEW,
    JDK_28,
    JDK_28_PREVIEW,
    JDK_X,
}

impl LanguageLevel {
    /// `LanguageLevel.HIGHEST`.
    pub const HIGHEST: LanguageLevel = LanguageLevel::JDK_28;

    /// Every level, in declaration order.
    pub const ALL: [LanguageLevel; 39] = {
        use LanguageLevel::*;
        [
            JDK_1_3,
            JDK_1_4,
            JDK_1_5,
            JDK_1_6,
            JDK_1_7,
            JDK_1_8,
            JDK_1_9,
            JDK_10,
            JDK_11,
            JDK_12,
            JDK_13,
            JDK_14,
            JDK_15,
            JDK_16,
            JDK_17,
            JDK_17_PREVIEW,
            JDK_18,
            JDK_18_PREVIEW,
            JDK_19,
            JDK_19_PREVIEW,
            JDK_20,
            JDK_20_PREVIEW,
            JDK_21,
            JDK_21_PREVIEW,
            JDK_22,
            JDK_22_PREVIEW,
            JDK_23,
            JDK_23_PREVIEW,
            JDK_24,
            JDK_24_PREVIEW,
            JDK_25,
            JDK_25_PREVIEW,
            JDK_26,
            JDK_26_PREVIEW,
            JDK_27,
            JDK_27_PREVIEW,
            JDK_28,
            JDK_28_PREVIEW,
            JDK_X,
        ]
    };

    /// `isAtLeast(level)`.
    #[inline]
    pub fn is_at_least(self, level: LanguageLevel) -> bool {
        self >= level
    }

    /// `isLessThan(level)`.
    #[inline]
    pub fn is_less_than(self, level: LanguageLevel) -> bool {
        self < level
    }

    /// `isPreview`: a `_PREVIEW` level or `JDK_X`.
    pub fn is_preview(self) -> bool {
        self.name().ends_with("_PREVIEW") || self == LanguageLevel::JDK_X
    }

    /// `isUnsupported`: the obsolete preview levels 17 to 20.
    pub fn is_unsupported(self) -> bool {
        use LanguageLevel::*;
        matches!(self, JDK_17_PREVIEW | JDK_18_PREVIEW | JDK_19_PREVIEW | JDK_20_PREVIEW)
    }

    /// The constant's name (`JDK_1_8`).
    pub fn name(self) -> &'static str {
        use LanguageLevel::*;
        match self {
            JDK_1_3 => "JDK_1_3",
            JDK_1_4 => "JDK_1_4",
            JDK_1_5 => "JDK_1_5",
            JDK_1_6 => "JDK_1_6",
            JDK_1_7 => "JDK_1_7",
            JDK_1_8 => "JDK_1_8",
            JDK_1_9 => "JDK_1_9",
            JDK_10 => "JDK_10",
            JDK_11 => "JDK_11",
            JDK_12 => "JDK_12",
            JDK_13 => "JDK_13",
            JDK_14 => "JDK_14",
            JDK_15 => "JDK_15",
            JDK_16 => "JDK_16",
            JDK_17 => "JDK_17",
            JDK_17_PREVIEW => "JDK_17_PREVIEW",
            JDK_18 => "JDK_18",
            JDK_18_PREVIEW => "JDK_18_PREVIEW",
            JDK_19 => "JDK_19",
            JDK_19_PREVIEW => "JDK_19_PREVIEW",
            JDK_20 => "JDK_20",
            JDK_20_PREVIEW => "JDK_20_PREVIEW",
            JDK_21 => "JDK_21",
            JDK_21_PREVIEW => "JDK_21_PREVIEW",
            JDK_22 => "JDK_22",
            JDK_22_PREVIEW => "JDK_22_PREVIEW",
            JDK_23 => "JDK_23",
            JDK_23_PREVIEW => "JDK_23_PREVIEW",
            JDK_24 => "JDK_24",
            JDK_24_PREVIEW => "JDK_24_PREVIEW",
            JDK_25 => "JDK_25",
            JDK_25_PREVIEW => "JDK_25_PREVIEW",
            JDK_26 => "JDK_26",
            JDK_26_PREVIEW => "JDK_26_PREVIEW",
            JDK_27 => "JDK_27",
            JDK_27_PREVIEW => "JDK_27_PREVIEW",
            JDK_28 => "JDK_28",
            JDK_28_PREVIEW => "JDK_28_PREVIEW",
            JDK_X => "JDK_X",
        }
    }

    /// `LanguageLevel.valueOf(name)`.
    pub fn from_name(name: &str) -> Option<LanguageLevel> {
        LanguageLevel::ALL.iter().copied().find(|l| l.name() == name)
    }
}

/// The `JavaFeature`s the java-syntax lexers, binders and parsers ask about, with their minimum level.
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum JavaFeature {
    LVTI,
    VAR_LAMBDA_PARAMETER,
    SWITCH_EXPRESSION,
    RECORDS,
    SEALED_CLASSES,
    PATTERNS_IN_SWITCH,
    MARKDOWN_COMMENT,
    VALHALLA_VALUE_CLASSES,
}

impl JavaFeature {
    /// `minimumLevel`.
    pub fn minimum_level(self) -> LanguageLevel {
        use LanguageLevel::*;
        match self {
            JavaFeature::LVTI => JDK_10,
            JavaFeature::VAR_LAMBDA_PARAMETER => JDK_11,
            JavaFeature::SWITCH_EXPRESSION => JDK_14,
            JavaFeature::RECORDS => JDK_16,
            JavaFeature::SEALED_CLASSES => JDK_17,
            JavaFeature::PATTERNS_IN_SWITCH => JDK_21,
            JavaFeature::MARKDOWN_COMMENT => JDK_23,
            JavaFeature::VALHALLA_VALUE_CLASSES => JDK_X,
        }
    }

    /// `myObsoletePreviewLevels`.
    fn is_obsolete_preview_level(self, level: LanguageLevel) -> bool {
        use LanguageLevel::*;
        self == JavaFeature::PATTERNS_IN_SWITCH
            && matches!(level, JDK_17_PREVIEW | JDK_18_PREVIEW | JDK_19_PREVIEW | JDK_20_PREVIEW)
    }

    /// `isSufficient(useSiteLevel)`: none of these features overrides it.
    pub fn is_sufficient(self, use_site_level: LanguageLevel) -> bool {
        let minimum = self.minimum_level();
        (use_site_level.is_at_least(minimum)
            || use_site_level.is_unsupported() && self.is_obsolete_preview_level(use_site_level))
            && (!minimum.is_preview() || use_site_level.is_preview())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_in_declaration_order() {
        for (i, l) in LanguageLevel::ALL.iter().enumerate() {
            assert_eq!(*l as usize, i);
            assert_eq!(LanguageLevel::from_name(l.name()), Some(*l));
        }
        assert!(JavaFeature::VALHALLA_VALUE_CLASSES.is_sufficient(LanguageLevel::JDK_X));
        assert!(!JavaFeature::VALHALLA_VALUE_CLASSES.is_sufficient(LanguageLevel::JDK_28));
        assert!(JavaFeature::PATTERNS_IN_SWITCH.is_sufficient(LanguageLevel::JDK_20_PREVIEW));
        assert!(!JavaFeature::PATTERNS_IN_SWITCH.is_sufficient(LanguageLevel::JDK_20));
    }
}
