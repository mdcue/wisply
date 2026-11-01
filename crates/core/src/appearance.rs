//! How the wisps look: keeps each wisp's icon files (see `icons`) in step with its colour and face.
//! The icons live in `<data folder>/icons/<wisp>/`, and the launcher and Hub read them from there.

use std::fs;
use std::path::Path;

use rusqlite::Connection;

use crate::icons::{self, Shape};
use crate::store::{self, WispChanges};
use crate::{Error, Result};

/// The file that remembers what an icon was drawn from, so a changed colour or face (or shape) is
/// noticed and the icon redrawn.
const LOOK_FILE: &str = "look.txt";

fn look_text(expression: &str, color: &str, shape: Shape) -> String {
    format!("{expression}|{}|{}", color.trim().to_lowercase(), shape.name())
}

/// Draws and saves one wisp's two icons, and remembers what they were drawn from.
fn draw(data_dir: &Path, wisp_id: &str, expression: &str, color: &str) -> Result<()> {
    let shape = Shape::for_wisp(wisp_id);
    let made = |error: icons::IconError| Error::Invalid(format!("icon for {wisp_id}: {error}"));
    let icon = icons::generate(expression, color, shape).map_err(made)?;
    icons::write_icons(data_dir, wisp_id, &icon, shape).map_err(made)?;
    let folder = icons::folder(data_dir, wisp_id).map_err(made)?;
    fs::write(folder.join(LOOK_FILE), look_text(expression, color, shape))?;
    Ok(())
}

/// Makes every wisp's icons that are missing or out of date: the files aren't there, or the wisp's
/// colour, face or shape is no longer what they were drawn from. Wisps whose icons are fine are
/// left alone, so this is cheap to call often. A wisp whose icon can't be drawn is logged and
/// skipped, and doesn't stop the others. Gives back how many wisps were redrawn.
pub fn ensure_icons(conn: &Connection, data_dir: &Path) -> Result<usize> {
    let mut redrawn = 0;
    for wisp in store::all_wisps(conn)? {
        let folder = match icons::folder(data_dir, &wisp.id) {
            Ok(folder) => folder,
            Err(error) => {
                log::warn!("no icon for wisp {:?}: {error}", wisp.id);
                continue;
            }
        };
        let wanted = look_text(&wisp.expression, &wisp.color, Shape::for_wisp(&wisp.id));
        let up_to_date = folder.join("icon.png").is_file()
            && folder.join("icon_badge.png").is_file()
            && fs::read_to_string(folder.join(LOOK_FILE)).map(|text| text == wanted).unwrap_or(false);
        if up_to_date {
            continue;
        }
        match draw(data_dir, &wisp.id, &wisp.expression, &wisp.color) {
            Ok(()) => redrawn += 1,
            Err(error) => log::warn!("could not draw the icon for {}: {error}", wisp.id),
        }
    }
    Ok(redrawn)
}

/// Changes a wisp's colour and/or face and redraws its icons. The icons are drawn first, so a bad
/// colour or an unknown face is refused and nothing is changed. Anything left as `None` stays.
pub fn set_appearance(
    conn: &Connection,
    data_dir: &Path,
    wisp_id: &str,
    color: Option<&str>,
    expression: Option<&str>,
) -> Result<()> {
    let wisp = store::get_wisp(conn, wisp_id)?.ok_or_else(|| Error::Invalid(format!("no wisp called {wisp_id}")))?;
    let color = color.unwrap_or(&wisp.color).to_string();
    let expression = expression.unwrap_or(&wisp.expression).to_string();
    draw(data_dir, wisp_id, &expression, &color)?;
    store::update_wisp(
        conn,
        wisp_id,
        &WispChanges { color: Some(color), expression: Some(expression), ..Default::default() },
    )
}
