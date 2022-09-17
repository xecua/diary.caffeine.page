use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Context;
use handlebars::{handlebars_helper, Handlebars};

handlebars_helper!(breadcrumbs: |path: PathBuf| {
    let mut current_path = PathBuf::from("/");
    let mut res = String::new();
    let mut components = path.components();
    if path.has_root() {
        components.next();
    }
    res.push_str("<a href=\"/\">/</a> ");
    for (i, c) in components.enumerate() {
        current_path.push(c);
        let _ = write!(
            res,
            "{}<a href=\"{}\">{}</a>",
            if i == 0 {""} else {" / "},
            current_path.to_string_lossy(),
            current_path.file_stem().unwrap().to_string_lossy() // file_prefix: unstable
        );
    }

    res
});

handlebars_helper!(slice_until: |lst: array, upper: usize| lst[..upper].to_owned());
handlebars_helper!(slice_since: |lst: array, lower: usize| lst[lower..].to_owned());
handlebars_helper!(slice: |lst: array, lower: usize, upper: usize| lst[lower..upper].to_owned());

pub(super) fn generate_renderer(template_dir: &Path) -> anyhow::Result<Handlebars<'static>> {
    let mut handlebars = handlebars::Handlebars::new();
    handlebars.register_helper("breadcrumbs", Box::new(breadcrumbs));
    handlebars.register_helper("slice", Box::new(slice));
    handlebars.register_helper("slice_since", Box::new(slice_since));
    handlebars.register_helper("slice_until", Box::new(slice_until));
    handlebars
        .register_template_file("index", template_dir.join("index.hbs"))
        .context("index.hbs")?;
    handlebars
        .register_template_file("article", template_dir.join("article.hbs"))
        .context("article.hbs")?;
    handlebars
        .register_template_file("list", template_dir.join("list.hbs"))
        .context("list.hbs")?;
    handlebars.register_partial(
        "layout",
        std::fs::read_to_string(template_dir.join("layout.hbs")).context("header.hbs")?,
    )?;

    Ok(handlebars)
}
