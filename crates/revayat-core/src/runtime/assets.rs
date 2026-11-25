use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub(crate) fn find_file(relative: &Path) -> Option<PathBuf> {
    let executable = env::current_exe().ok();
    let launcher = executable.as_deref().and_then(Path::parent);
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    launcher
        .into_iter()
        .flat_map(Path::ancestors)
        .chain(source.ancestors())
        .find_map(|root| {
            let direct = root.join(relative);
            let installed = root.join("skills/revayat-subtitle").join(relative);
            [direct, installed]
                .into_iter()
                .find(|p| {
                    fs::symlink_metadata(p).is_ok_and(|m| m.is_file() && !super::files::linked(&m))
                })
                .and_then(|p| fs::canonicalize(p).ok())
        })
}

pub fn bundled_fonts_dir() -> Option<PathBuf> {
    find_file(Path::new("assets/fonts/BKOODB.TTF"))
        .and_then(|font| font.parent().map(Path::to_path_buf))
}
