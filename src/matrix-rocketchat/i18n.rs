use yaml_rust::{Yaml, YamlLoader};

lazy_static! {
    /// Translations for the application service
    pub static ref TRANSLATIONS: Vec<Yaml> = {
        let translations = include_str!("../../assets/translations.yaml");
        YamlLoader::load_from_str(translations).expect("Could not load translations")
    };
}

macro_rules! build_i18n_struct {
    ($($f:ident),*) => {
        /// Struct that stores translations for all supported languages
        pub struct I18n {
            $(
                /// language field
                pub $f: String
             ),*
        }

        impl I18n {
            /// Return the translation for a language
            pub fn l(&self, language: &str) -> String {
                match language{
                    $(stringify!($f) => self.$f.clone(),)*
                        _ => "Unsupported language".to_string()
                }
            }
        }
    }
}

macro_rules! translate_all_languages {
    ($($language:ident => [[$($key:expr),*]; [$($k:expr => $v:expr),*]]);*)  =>{
        {
            I18n {
                $($language:
                  {
                      let translation = &TRANSLATIONS[0][stringify!($language)]$([$key])*;
                      match translation.as_str() {
                          Some(value) => value.to_string()$(.replace(concat!("{", $k, "}"),$v))*,
                          None => format!("Translation '{}' not found", [$($key),*].join("."))
                      }
                  }
                 ),+
            }
        }
    };
}

macro_rules! setup_translation_macro {
    ($($l:ident),*) => {
        macro_rules! t {
            ($keys:tt;$repl:tt) => {
                {
                    translate_all_languages!($($l => [$keys; $repl]);*)
                }
            };
            ($keys:tt) => {
                {
                    translate_all_languages!($($l => [$keys;[]]);*)
                }
            }
        }
    };
}

macro_rules! i18n_languages {
    ($($l:ident),*) => {
        build_i18n_struct!($($l),*);
        setup_translation_macro!($($l),*);
    }
}

/// A list of languages that are supported by the application bridge
i18n_languages!(en);

/// Language that is used if no language is specified
pub const DEFAULT_LANGUAGE: &'static str = "en";
