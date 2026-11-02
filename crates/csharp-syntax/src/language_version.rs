//! `LanguageVersion` (Roslyn's `LanguageVersion.cs`): the effective versions a parse runs at, `TryParse` of the names a
//! list line carries, `ToDisplayString`, `GetErrorCode` and the feature checks of `CSharpParseOptions.IsFeatureEnabled`.

use crate::error_code::{ErrorCode, MessageID};

/// A C# language version after `MapSpecifiedToEffectiveVersion`: `Latest`, `LatestMajor` and `Default` are
/// `CSharp14` (`LanguageVersion::LATEST`), what `CSharpParseOptions.LanguageVersion` reports for them. Ordered by
/// Roslyn's numbers, so `version >= feature.required_version()` compares as `IsFeatureEnabled` does.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(u32)]
pub enum LanguageVersion {
    CSharp1 = 1,
    CSharp2 = 2,
    CSharp3 = 3,
    CSharp4 = 4,
    CSharp5 = 5,
    CSharp6 = 6,
    CSharp7 = 7,
    CSharp7_1 = 701,
    CSharp7_2 = 702,
    CSharp7_3 = 703,
    CSharp8 = 800,
    CSharp9 = 900,
    CSharp10 = 1000,
    CSharp11 = 1100,
    CSharp12 = 1200,
    CSharp13 = 1300,
    CSharp14 = 1400,
    Preview = i32::MAX as u32 - 1,
}

impl LanguageVersion {
    /// `LanguageVersion.Latest` (and `Default`, `LatestMajor`) mapped to its effective version
    /// (`LanguageVersionFacts.CurrentVersion`).
    pub const LATEST: LanguageVersion = LanguageVersion::CSharp14;

    /// `LanguageVersionFacts.TryParse` followed by `MapSpecifiedToEffectiveVersion`: `latest`, `preview`, `default`,
    /// `latestmajor`, `7.3`, `iso-1`, ... (case-insensitive); `None` for a name that is no version.
    pub fn from_name(name: &str) -> Option<LanguageVersion> {
        use LanguageVersion::*;
        Some(match name.to_ascii_lowercase().as_str() {
            "default" | "latest" | "latestmajor" => LanguageVersion::LATEST,
            "preview" => Preview,
            "1" | "1.0" | "iso-1" => CSharp1,
            "2" | "2.0" | "iso-2" => CSharp2,
            "3" | "3.0" => CSharp3,
            "4" | "4.0" => CSharp4,
            "5" | "5.0" => CSharp5,
            "6" | "6.0" => CSharp6,
            "7" | "7.0" => CSharp7,
            "7.1" => CSharp7_1,
            "7.2" => CSharp7_2,
            "7.3" => CSharp7_3,
            "8" | "8.0" => CSharp8,
            "9" | "9.0" => CSharp9,
            "10" | "10.0" => CSharp10,
            "11" | "11.0" => CSharp11,
            "12" | "12.0" => CSharp12,
            "13" | "13.0" => CSharp13,
            "14" | "14.0" => CSharp14,
            _ => return None,
        })
    }

    /// `LanguageVersionFacts.ToDisplayString`: "7.3", "14.0", "preview" (what `CSharpRequiredLanguageVersion` prints).
    pub fn to_display_string(self) -> &'static str {
        use LanguageVersion::*;
        match self {
            CSharp1 => "1",
            CSharp2 => "2",
            CSharp3 => "3",
            CSharp4 => "4",
            CSharp5 => "5",
            CSharp6 => "6",
            CSharp7 => "7.0",
            CSharp7_1 => "7.1",
            CSharp7_2 => "7.2",
            CSharp7_3 => "7.3",
            CSharp8 => "8.0",
            CSharp9 => "9.0",
            CSharp10 => "10.0",
            CSharp11 => "11.0",
            CSharp12 => "12.0",
            CSharp13 => "13.0",
            CSharp14 => "14.0",
            Preview => "preview",
        }
    }

    /// `LanguageVersionExtensionsInternal.GetErrorCode`: the "not available in version N" error of this version (Roslyn
    /// throws for `Preview`, where every feature is available; `None` here).
    pub fn get_error_code(self) -> Option<ErrorCode> {
        use LanguageVersion::*;
        Some(match self {
            CSharp1 => ErrorCode::ERR_FeatureNotAvailableInVersion1,
            CSharp2 => ErrorCode::ERR_FeatureNotAvailableInVersion2,
            CSharp3 => ErrorCode::ERR_FeatureNotAvailableInVersion3,
            CSharp4 => ErrorCode::ERR_FeatureNotAvailableInVersion4,
            CSharp5 => ErrorCode::ERR_FeatureNotAvailableInVersion5,
            CSharp6 => ErrorCode::ERR_FeatureNotAvailableInVersion6,
            CSharp7 => ErrorCode::ERR_FeatureNotAvailableInVersion7,
            CSharp7_1 => ErrorCode::ERR_FeatureNotAvailableInVersion7_1,
            CSharp7_2 => ErrorCode::ERR_FeatureNotAvailableInVersion7_2,
            CSharp7_3 => ErrorCode::ERR_FeatureNotAvailableInVersion7_3,
            CSharp8 => ErrorCode::ERR_FeatureNotAvailableInVersion8,
            CSharp9 => ErrorCode::ERR_FeatureNotAvailableInVersion9,
            CSharp10 => ErrorCode::ERR_FeatureNotAvailableInVersion10,
            CSharp11 => ErrorCode::ERR_FeatureNotAvailableInVersion11,
            CSharp12 => ErrorCode::ERR_FeatureNotAvailableInVersion12,
            CSharp13 => ErrorCode::ERR_FeatureNotAvailableInVersion13,
            CSharp14 => ErrorCode::ERR_FeatureNotAvailableInVersion14,
            Preview => return None,
        })
    }

    /// `CSharpParseOptions.IsFeatureEnabled`: no feature has a `RequiredFeature` flag at the pin, so it is
    /// `LanguageVersion >= feature.RequiredVersion()` (a feature Roslyn has no version for counts as enabled).
    pub fn is_feature_enabled(self, feature: MessageID) -> bool {
        feature.required_version().is_none_or(|required| self >= required)
    }
}

impl Default for LanguageVersion {
    /// `LanguageVersion.Latest`, as `CSharpParseOptions.Default` parses.
    fn default() -> Self {
        LanguageVersion::LATEST
    }
}
