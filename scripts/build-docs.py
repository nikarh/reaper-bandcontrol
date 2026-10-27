#!/usr/bin/env python3
"""Renders docs/features.toml into the README and the website.

    build-docs.py readme      rewrite the README section between the `features` markers
    build-docs.py check       fail if the README is outdated or a referenced screenshot is missing
    build-docs.py site DIR    build the website (site/ + docs/screenshots + changelog) into DIR
    build-docs.py icon        regenerate the website's icons from the app's launcher icon (needs ImageMagick)
    build-docs.py fastlane    write fastlane/metadata/android (the F-Droid listing) from features.toml and screenshots
"""

import html
import re
import shutil
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
README = ROOT / "README.md"
SCREENSHOTS = ROOT / "docs" / "screenshots"
REPO = "https://github.com/nikarh/reaper-bandcontrol"
START, END = "<!-- features:start -->", "<!-- features:end -->"


def features() -> list[dict]:
    return tomllib.loads((ROOT / "docs" / "features.toml").read_text())["feature"]


def version() -> str:
    text = (ROOT / "Cargo.toml").read_text()
    section = text.split("[workspace.package]", 1)[1]
    return re.search(r'^version\s*=\s*"([^"]+)"', section, re.M).group(1)


def reaper(key: str) -> str:
    """`min` or `tested` from [workspace.metadata.reaper] in Cargo.toml."""
    text = (ROOT / "Cargo.toml").read_text()
    section = text.split("[workspace.metadata.reaper]", 1)[1].split("\n[", 1)[0]
    return re.search(rf'^{key}\s*=\s*"([^"]+)"', section, re.M).group(1)


def readme_reaper_versions() -> list[str]:
    """Problems with the REAPER versions the README names: they must be the ones in Cargo.toml."""
    allowed = {reaper("min"), reaper("tested")}
    found = set()
    for line in README.read_text().splitlines():
        if "REAPER" in line:
            found |= set(re.findall(r"(?<![\d.])[4-9]\.\d{1,2}\b", line))
    problems = [f"README names REAPER {v}, Cargo.toml says min {reaper('min')}, tested {reaper('tested')}"
                for v in sorted(found - allowed)]
    problems += [f"README doesn't name REAPER {v} ([workspace.metadata.reaper] in Cargo.toml)"
                 for v in sorted(allowed - found)]
    return problems


def macos_min() -> str:
    """The oldest supported macOS, MACOSX_DEPLOYMENT_TARGET in .cargo/config.toml, as people write it ("11")."""
    text = (ROOT / ".cargo" / "config.toml").read_text()
    version = re.search(r'^MACOSX_DEPLOYMENT_TARGET\s*=\s*"([^"]+)"', text, re.M).group(1)
    return version.removesuffix(".0")


def readme_macos_version() -> list[str]:
    """The README names the oldest supported macOS and no older one."""
    text = README.read_text()
    problems = []
    if f"macOS {macos_min()} " not in text:
        problems.append(f"README doesn't name macOS {macos_min()} (MACOSX_DEPLOYMENT_TARGET in .cargo/config.toml)")
    for v in sorted(set(re.findall(r"macOS (\d+(?:\.\d+)?)", text))):
        if float(v) < float(macos_min()):
            problems.append(f"README names macOS {v}, older than the supported {macos_min()}")
    return problems


def is_app_screenshot(name: str) -> bool:
    return not name.startswith("reaper-")


def paragraph(text: str) -> str:
    return " ".join(text.split())


def readme_section() -> str:
    out = [START, "<!-- Generated from docs/features.toml by scripts/build-docs.py, edit there. -->", ""]
    for f in features():
        out.append(f"### {f['title']}")
        out.append("")
        shot = f.get("screenshot")
        if shot and is_app_screenshot(shot):
            out.append(f'<img src="docs/screenshots/{shot}.png" alt="{f["title"]}" width="220" align="right">')
            out.append("")
        out.append(paragraph(f["text"]))
        out.append("")
        if shot and not is_app_screenshot(shot):
            out.append(f'<img src="docs/screenshots/{shot}.png" alt="{f["title"]}">')
            out.append("")
        if shot and is_app_screenshot(shot):
            out.append('<br clear="right">')
            out.append("")
    out.append(END)
    return "\n".join(out)


def render_readme() -> str:
    text = README.read_text()
    start, end = text.index(START), text.index(END) + len(END)
    return text[:start] + readme_section() + text[end:]


def inline(text: str) -> str:
    text = html.escape(text)
    text = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", text)
    text = re.sub(r"`(.+?)`", r"<code>\1</code>", text)
    return re.sub(r"\[(.+?)\]\((.+?)\)", r'<a href="\2">\1</a>', text)


def changelog_html(releases: int = 3) -> str:
    """The newest sections of CHANGELOG.md as HTML (headings and lists only)."""
    path = ROOT / "CHANGELOG.md"
    if not path.exists():
        return ""
    sections = re.split(r"^## ", path.read_text(), flags=re.M)[1 : releases + 1]
    out = ['<div class="changes">']
    for section in sections:
        lines = section.strip().splitlines()
        out.append(f"<h3>{inline(lines[0])}</h3>")
        in_list = False
        for line in lines[1:]:
            if line.startswith("### "):
                if in_list:
                    out.append("</ul>")
                    in_list = False
                out.append(f"<h4>{inline(line[4:])}</h4>")
            elif line.startswith("- "):
                if not in_list:
                    out.append("<ul>")
                    in_list = True
                out.append(f"<li>{inline(line[2:])}</li>")
            elif line.startswith("[") or line.startswith("<!--"):
                continue
        if in_list:
            out.append("</ul>")
    out.append("</div>")
    return "\n".join(out)


def features_html() -> str:
    out = []
    for f in features():
        shot = f.get("screenshot")
        image = ""
        if shot:
            cls = "" if is_app_screenshot(shot) else ' class="wide"'
            src = f"screenshots/{shot}.png"
            image = (
                f'<a class="zoom" href="{src}"><img src="{src}" alt="{html.escape(f["title"])}"{cls} loading="lazy"></a>'
            )
        out.append(
            f'<article class="feature" id="{f["id"]}"><div class="text"><h3>{html.escape(f["title"])}</h3>'
            f"<p>{inline(paragraph(f['text']))}</p></div>{image}</article>"
        )
    return "\n".join(out)


ANDROID_RES = ROOT / "android" / "app" / "src" / "main" / "res"
APP_ICON = ROOT / "site" / "app-icon.svg"


def app_icon_svg(rounded: bool = True) -> str:
    """The app's adaptive launcher icon (foreground drawable on the launcher background color) as SVG, cropped to
    the 72 dp a launcher shows. `rounded` gives it corners; iOS rounds home screen icons itself."""
    colors = (ANDROID_RES / "values" / "colors.xml").read_text()
    background = "#" + re.search(r'name="launcher_background">#FF(\w{6})<', colors).group(1)
    vector = (ANDROID_RES / "drawable" / "ic_launcher_foreground.xml").read_text()
    paths = [
        f'<path fill="#{color}" d="{data}"/>'
        for color, data in re.findall(r'android:fillColor="#FF(\w{6})"\s+android:pathData="([^"]+)"', vector)
    ]
    corners = ' rx="16"' if rounded else ""
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="18 18 72 72">'
        f'<rect x="18" y="18" width="72" height="72"{corners} fill="{background}"/>' + "".join(paths) + "</svg>\n"
    )


FASTLANE = ROOT / "fastlane" / "metadata" / "android" / "en-US"
WEBSITE = "https://bandcontrol.arhipov.net/"


def app_listing() -> dict:
    return tomllib.loads((ROOT / "docs" / "features.toml").read_text())["app"]


def android_version_code() -> int:
    text = (ROOT / "Cargo.toml").read_text()
    return int(re.search(r"^version-code\s*=\s*(\d+)", text, re.M).group(1))


def fastlane_texts() -> dict[str, str]:
    """The text files of the listing, by path relative to FASTLANE."""
    app = app_listing()
    short = app["short"].strip()
    assert len(short) <= 80 and not short.endswith("."), f"short description: at most 80 characters, no dot: {short!r}"
    needs = paragraph(app["needs"]).format(min=reaper("min"), website=WEBSITE)
    parts = [paragraph(app["intro"]), needs]
    for f in features():
        parts.append(f"{f['title']}\n{paragraph(f['text'])}")
    full = "\n\n".join(parts) + "\n"
    assert len(full) <= 4000, f"full description: {len(full)} characters, at most 4000"
    texts = {"title.txt": app["title"] + "\n", "short_description.txt": short + "\n", "full_description.txt": full}
    notes = app_release_notes()
    if notes is not None:
        texts[f"changelogs/{android_version_code()}.txt"] = notes
    return texts


CHANGELOG_URL = f"{REPO}/blob/main/CHANGELOG.md"
APP_SCOPES = {"android"}
APP_GROUPS = {"Features", "Fixes", "Performance"}


def app_release_notes() -> str | None:
    """What's new in the app in this version (the store's release notes: plain text, at most 500 characters), from
    its section of CHANGELOG.md, once released: the features, fixes and performance changes of the app (scope
    `android`), without scopes and authors. None before the version is in the changelog."""
    changelog = (ROOT / "CHANGELOG.md").read_text() if (ROOT / "CHANGELOG.md").exists() else ""
    sections = re.findall(r"^## \[([^\]]+)\][^\n]*\n(.*?)(?=^## |\Z)", changelog, re.M | re.S)
    released = [v for v, _ in sections if v != "Unreleased"]
    body = next((b for v, b in sections if v == version()), None)
    if body is None:
        return None
    if released and released[-1] == version():
        return "First release.\n"
    items, group = [], ""
    for line in body.splitlines():
        if line.startswith("### "):
            group = line[4:].strip()
        entry = re.match(r"- \*\*([^*]+):\*\* (.*?)(?: by @\S+)?(?: in #\d+)?$", line)
        if entry and group in APP_GROUPS and entry.group(1) in APP_SCOPES:
            items.append(entry.group(2))
    if not items:
        return "No changes in the app in this version.\n"
    more = f"More in the changelog: {CHANGELOG_URL}\n"
    notes = ""
    for i, item in enumerate(items):
        line = f"• {item}\n"
        room = 500 if i == len(items) - 1 else 500 - len(more)
        if len(notes) + len(line) > room:
            return notes + more
        notes += line
    return notes


def fastlane_screenshots() -> list[tuple[Path, str]]:
    """(source, name) of the phone screenshots, in the order of the features."""
    shots = [f["screenshot"] for f in features() if f.get("screenshot") and is_app_screenshot(f["screenshot"])]
    return [(SCREENSHOTS / f"{s}.png", f"{i}_{s}.png") for i, s in enumerate(shots, 1)]


def build_fastlane() -> None:
    for name, text in fastlane_texts().items():
        (FASTLANE / name).parent.mkdir(parents=True, exist_ok=True)
        (FASTLANE / name).write_text(text)
    phones = FASTLANE / "images" / "phoneScreenshots"
    if phones.exists():
        shutil.rmtree(phones)
    phones.mkdir(parents=True)
    for src, name in fastlane_screenshots():
        shutil.copyfile(src, phones / name)
    print(f"fastlane metadata written to {FASTLANE.relative_to(ROOT)}")


def fastlane_problems() -> list[str]:
    problems = []
    for name, text in fastlane_texts().items():
        if not (FASTLANE / name).exists() or (FASTLANE / name).read_text() != text:
            problems.append(f"fastlane/…/{name} is outdated, run `mise run docs`")
    phones = FASTLANE / "images" / "phoneScreenshots"
    expected = {name: src for src, name in fastlane_screenshots()}
    present = {p.name for p in phones.glob("*.png")} if phones.exists() else set()
    if present != set(expected) or any((phones / n).read_bytes() != s.read_bytes() for n, s in expected.items()):
        problems.append("fastlane phone screenshots differ from docs/screenshots, run `mise run docs`")
    if not (FASTLANE / "images" / "icon.png").exists():
        problems.append("fastlane icon missing, run `python3 scripts/build-docs.py icon`")
    return problems


def build_icons() -> None:
    """site/app-icon.svg and its PNGs: apple-touch-icon.png (square, 180 px) and favicon-32.png; the 512 px store
    icon of the F-Droid listing."""
    import subprocess
    import tempfile

    APP_ICON.write_text(app_icon_svg())
    magick = shutil.which("magick") or shutil.which("convert")
    with tempfile.TemporaryDirectory() as tmp:
        square = Path(tmp) / "square.svg"
        square.write_text(app_icon_svg(rounded=False))
        (FASTLANE / "images").mkdir(parents=True, exist_ok=True)
        for src, size, out in [
            (square, 180, ROOT / "site" / "apple-touch-icon.png"),
            (APP_ICON, 32, ROOT / "site" / "favicon-32.png"),
            (square, 512, FASTLANE / "images" / "icon.png"),
        ]:
            subprocess.run([magick, "-background", "none", "-density", "600", str(src), "-resize", f"{size}x{size}",
                            "-strip", str(out)], check=True)
    print("icons written to site/")


ICONS = ROOT / "site" / "icons"
ICON = re.compile(r"\{\{icon:([\w-]+)\}\}")


def icon(name: str) -> str:
    """An icon from site/icons, inlined so it takes the text color; decorative, the text next to it says it all."""
    svg = (ICONS / f"{name}.svg").read_text().strip()
    return svg.replace("<svg ", '<svg class="icon" aria-hidden="true" focusable="false" ', 1)


def build_site(out: Path) -> None:
    if out.exists():
        shutil.rmtree(out)
    shutil.copytree(ROOT / "site", out)
    shutil.copytree(SCREENSHOTS, out / "screenshots")
    shutil.copytree(ROOT / "examples", out / "examples")
    page = (out / "index.html").read_text()
    for key, value in {
        "features": features_html(),
        "changelog": changelog_html(),
        "version": version(),
        "reaper_min": reaper("min"),
        "reaper_tested": reaper("tested"),
        "macos_min": macos_min(),
        "repo": REPO,
        "repo_path": REPO.removeprefix("https://github.com/"),
    }.items():
        page = page.replace("{{" + key + "}}", value)
    page = ICON.sub(lambda m: icon(m.group(1)), page)
    (out / "index.html").write_text(page)
    print(f"site built in {out}")


def missing_screenshots() -> list[str]:
    names = {f["screenshot"] for f in features() if f.get("screenshot")}
    names |= set(re.findall(r"screenshots/([\w-]+)\.png", (ROOT / "site" / "index.html").read_text()))
    return sorted(n for n in names if not (SCREENSHOTS / f"{n}.png").exists())


def main() -> int:
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    if command == "readme":
        README.write_text(render_readme())
    elif command == "check":
        problems = [f"missing screenshot docs/screenshots/{n}.png" for n in missing_screenshots()]
        site = (ROOT / "site" / "index.html").read_text()
        problems += readme_reaper_versions()
        problems += readme_macos_version()
        problems += fastlane_problems()
        if not APP_ICON.exists() or APP_ICON.read_text() != app_icon_svg():
            problems.append("site/app-icon.svg differs from the app's launcher icon, run `python3 scripts/build-docs.py icon`")
        problems += [f"missing icon site/icons/{n}.svg" for n in sorted(set(ICON.findall(site))) if not (ICONS / f"{n}.svg").exists()]
        if README.read_text() != render_readme():
            problems.append("README.md feature section is outdated, run `mise run docs`")
        for p in problems:
            print(p, file=sys.stderr)
        return 1 if problems else 0
    elif command == "icon":
        build_icons()
    elif command == "fastlane":
        build_fastlane()
    elif command == "site" and len(sys.argv) == 3:
        build_site(Path(sys.argv[2]))
    else:
        print(__doc__, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
