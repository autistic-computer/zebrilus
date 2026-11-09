use std::{
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

use zz_terminal::{
    AppearanceConfigKey, AppearanceLoad, AppearanceSource, TerminalColorScheme, ThemeChoice,
    discover_ghostty_config, load_ghostty_appearance_from_for,
};

use crate as config;

#[derive(Debug, Default)]
pub struct ImportReport {
    pub ghostty_keys: usize,
    pub config_path: Option<PathBuf>,
}

impl ImportReport {
    pub fn imported_anything(&self) -> bool {
        self.config_path.is_some()
    }
}

pub fn import_ghostty_config(scheme: TerminalColorScheme) -> io::Result<ImportReport> {
    discover_ghostty_config().map_or_else(
        || Ok(ImportReport::default()),
        |path| import_ghostty_config_from(&path, scheme),
    )
}

pub fn import_ghostty_config_from(
    path: &Path,
    scheme: TerminalColorScheme,
) -> io::Result<ImportReport> {
    std::fs::File::open(path)
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))?;
    if !path.is_file() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("{}: expected a configuration file", path.display()),
        ));
    }
    let load = load_ghostty_appearance_from_for(path, scheme);
    if load.fatal {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("could not load {}: {:?}", path.display(), load.diagnostics),
        ));
    }
    let values = ghostty_import_values(&load)?;
    let mut report = ImportReport::default();
    if !values.is_empty() {
        let target = config::import_target_path()?;
        copy_theme_files(&load, path, &target)?;
        config::import_appearance_values_at(&target, &values)?;
        report.ghostty_keys = values.len();
        report.config_path = Some(target);
    }
    Ok(report)
}

pub fn copy_theme_files(
    load: &AppearanceLoad,
    ghostty_root: &Path,
    target: &Path,
) -> io::Result<()> {
    let Some(choice) = load
        .theme
        .as_deref()
        .and_then(|value| ThemeChoice::parse(value).ok())
    else {
        return Ok(());
    };
    let Some(directory) = target.parent().map(|parent| parent.join("themes")) else {
        return Ok(());
    };
    for name in choice.names() {
        if name.contains(['/', '\\']) {
            continue;
        }
        let Some(file) = zz_terminal::resolve_theme(name, ghostty_root) else {
            continue;
        };
        std::fs::create_dir_all(&directory)?;
        let copy = directory.join(name);
        if copy != file {
            std::fs::copy(&file, &copy)?;
        }
    }
    Ok(())
}

pub fn ghostty_import_values(
    load: &zz_terminal::AppearanceLoad,
) -> io::Result<Vec<(AppearanceConfigKey, Vec<String>)>> {
    let mut values = Vec::new();
    for key in AppearanceConfigKey::ALL {
        if key == AppearanceConfigKey::Theme {
            continue;
        }
        let source = load.provenance.source(key);
        if load.theme.is_some() && source == AppearanceSource::ThemeFile {
            values.push((key, Vec::new()));
            continue;
        }
        if !matches!(
            source,
            AppearanceSource::Ghostty | AppearanceSource::ThemeFile
        ) {
            continue;
        }
        let group = config::appearance_config_values(&load.appearance, key)
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error.to_string()))?;
        if group.is_empty() && !config::is_cumulative_appearance_key(key) {
            continue;
        }
        values.push((key, group));
    }
    if let Some(theme) = &load.theme {
        values.push((AppearanceConfigKey::Theme, vec![theme.clone()]));
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghostty_import_keeps_the_theme_line_instead_of_one_modes_colors() {
        let directory = tempfile::tempdir().unwrap();
        let themes = directory.path().join("themes");
        std::fs::create_dir(&themes).unwrap();
        std::fs::write(themes.join("day"), "background = #f0f1f2\n").unwrap();
        std::fs::write(themes.join("night"), "background = #101112\n").unwrap();
        let root = directory.path().join("config");
        std::fs::write(&root, "theme = light:day,dark:night\nfont-size = 15\n").unwrap();
        let load = zz_terminal::load_ghostty_appearance_from_for(&root, TerminalColorScheme::Light);
        let values = ghostty_import_values(&load).unwrap();
        let value = |key| {
            values
                .iter()
                .find(|(written, _)| *written == key)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(
            value(AppearanceConfigKey::Theme),
            Some(vec!["light:day,dark:night".to_owned()])
        );
        assert_eq!(value(AppearanceConfigKey::Background), Some(Vec::new()));
        assert_eq!(
            value(AppearanceConfigKey::FontSize),
            Some(vec!["15".to_owned()])
        );
    }

    #[test]
    fn ghostty_import_missing_donor_names_the_path() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing-ghostty");
        let error = import_ghostty_config_from(&missing, TerminalColorScheme::Dark).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
        assert!(error.to_string().contains(missing.to_str().unwrap()));
    }
}
