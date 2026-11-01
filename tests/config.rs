//! Cucumber features for loading the Lua config (`tests/features/config`).
#![expect(
    clippy::needless_pass_by_ref_mut,
    clippy::needless_pass_by_value,
    reason = "cucumber steps take the World as `&mut` and their captures by value"
)]

use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context as _, Result, anyhow, bail, ensure};
use barrs::config::{BarConfig, BarSection, Config, ItemConfig, load_config};
use barrs::source::{ItemSource, SystemPlugin};
use cucumber::gherkin::Step;
use cucumber::{Parameter, World as _, given, then, when};
use tempfile::TempDir;

/// One scenario: a config file in its own directory and the load result.
#[derive(Debug, cucumber::World)]
#[world(init = Self::new)]
struct ConfigWorld {
    dir: TempDir,
    loaded: Option<Result<Config, String>>,
}

impl ConfigWorld {
    fn new() -> std::io::Result<Self> {
        Ok(Self {
            dir: tempfile::tempdir()?,
            loaded: None,
        })
    }

    fn config_path(&self) -> PathBuf {
        self.dir.path().join("barrs.lua")
    }

    fn config(&self) -> Result<&Config> {
        match &self.loaded {
            Some(Ok(config)) => Ok(config),
            Some(Err(error)) => bail!("barrs rejected the config: {error}"),
            None => bail!("barrs has not loaded the config"),
        }
    }

    fn item(&self, id: &str) -> Result<&ItemConfig> {
        self.config()?
            .item(id)
            .with_context(|| format!("the config has no item {id}"))
    }
}

/// A numeric bar setting that a scenario can name in a table.
#[derive(Debug, Clone, Copy)]
enum BarSetting {
    Spacing,
    NotchWidth,
    NotchPadding,
    NotchOffset,
    NotchDisplayHeight,
}

impl BarSetting {
    const fn value(self, bar: &BarConfig) -> u32 {
        match self {
            Self::Spacing => bar.spacing,
            Self::NotchWidth => bar.notch_width,
            Self::NotchPadding => bar.notch_padding,
            Self::NotchOffset => bar.notch_offset,
            Self::NotchDisplayHeight => bar.notch_display_height,
        }
    }
}

impl FromStr for BarSetting {
    type Err = anyhow::Error;

    fn from_str(name: &str) -> Result<Self> {
        match name {
            "spacing" => Ok(Self::Spacing),
            "notch_width" => Ok(Self::NotchWidth),
            "notch_padding" => Ok(Self::NotchPadding),
            "notch_offset" => Ok(Self::NotchOffset),
            "notch_display_height" => Ok(Self::NotchDisplayHeight),
            other => Err(anyhow!("unknown bar setting {other}")),
        }
    }
}

/// A bar section as a scenario names it.
#[derive(Debug, Clone, Copy, Parameter)]
#[param(name = "section", regex = "left|middle|right")]
struct Section(BarSection);

impl FromStr for Section {
    type Err = anyhow::Error;

    fn from_str(name: &str) -> Result<Self> {
        match name {
            "left" => Ok(Self(BarSection::Left)),
            "middle" => Ok(Self(BarSection::Middle)),
            "right" => Ok(Self(BarSection::Right)),
            other => Err(anyhow!("unknown section {other}")),
        }
    }
}

/// The data rows of the table of `step`, without the header row.
fn table_rows(step: &Step) -> Result<impl Iterator<Item = &Vec<String>>> {
    let table = step.table.as_ref().context("the step needs a table")?;
    Ok(table.rows.iter().skip(1))
}

#[given("the config")]
fn the_config(world: &mut ConfigWorld, step: &Step) -> Result<()> {
    let text = step
        .docstring
        .as_deref()
        .context("the step needs the config as a doc string")?;
    fs::write(world.config_path(), text)?;
    Ok(())
}

#[when("barrs loads the config")]
fn barrs_loads_the_config(world: &mut ConfigWorld) {
    world.loaded = Some(load_config(&world.config_path()).map_err(|error| error.to_string()));
}

#[then(expr = "the config has {int} item(s)")]
fn the_config_has_items(world: &mut ConfigWorld, count: usize) -> Result<()> {
    let actual = world.config()?.items.len();
    ensure!(
        actual == count,
        "the config has {actual} items, not {count}"
    );
    Ok(())
}

#[then("the bar settings are")]
fn the_bar_settings_are(world: &mut ConfigWorld, step: &Step) -> Result<()> {
    let bar = &world.config()?.bar;
    for row in table_rows(step)? {
        let [setting, value] = row.as_slice() else {
            bail!("each row needs a setting and a value");
        };
        let setting: BarSetting = setting.parse()?;
        let expected: u32 = value.parse()?;
        let actual = setting.value(bar);
        ensure!(
            actual == expected,
            "{setting:?} is {actual}, not {expected}"
        );
    }
    Ok(())
}

#[then("the bar has no background")]
fn the_bar_has_no_background(world: &mut ConfigWorld) -> Result<()> {
    let background = &world.config()?.bar.background;
    ensure!(
        background.is_none(),
        "the bar has background {background:?}"
    );
    Ok(())
}

#[then(expr = "item {string} shows the time")]
fn item_shows_the_time(world: &mut ConfigWorld, id: String) -> Result<()> {
    let source = &world.item(&id)?.source;
    ensure!(
        *source == ItemSource::System(SystemPlugin::Time),
        "item {id} has source {source:?}"
    );
    Ok(())
}

#[then(expr = "item {string} shows the date in the format {string}")]
fn item_shows_the_date(world: &mut ConfigWorld, id: String, format: String) -> Result<()> {
    let source = &world.item(&id)?.source;
    ensure!(
        *source == ItemSource::System(SystemPlugin::Date { format }),
        "item {id} has source {source:?}"
    );
    Ok(())
}

#[then(expr = "item {string} is updated by the Lua function {string}")]
fn item_is_updated_by(world: &mut ConfigWorld, id: String, function: String) -> Result<()> {
    let update = world.item(&id)?.source.lua_update();
    ensure!(
        update == Some(function.as_str()),
        "item {id} has update function {update:?}"
    );
    Ok(())
}

#[then(expr = "item {string} is in the {section} section")]
fn item_is_in_section(world: &mut ConfigWorld, id: String, section: Section) -> Result<()> {
    let actual = BarSection::from_placement(world.item(&id)?.placement.as_deref());
    ensure!(
        actual == Some(section.0),
        "item {id} is in section {actual:?}, not {:?}",
        section.0
    );
    Ok(())
}

#[then("the config is rejected with an error that contains")]
fn the_config_is_rejected(world: &mut ConfigWorld, step: &Step) -> Result<()> {
    let error = match &world.loaded {
        Some(Err(error)) => error,
        Some(Ok(_)) => bail!("barrs accepted the config"),
        None => bail!("barrs has not loaded the config"),
    };
    for row in table_rows(step)? {
        let [text] = row.as_slice() else {
            bail!("each row needs one text");
        };
        ensure!(
            error.contains(text.as_str()),
            "{error:?} does not contain {text:?}"
        );
    }
    Ok(())
}

fn main() {
    // cargo-nextest lists tests with `--list`; Cucumber does not support it.
    if std::env::args().any(|arg| arg == "--list") {
        return;
    }
    futures::executor::block_on(
        ConfigWorld::cucumber()
            .fail_on_skipped()
            .run_and_exit(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/features/config"
            )),
    );
}
