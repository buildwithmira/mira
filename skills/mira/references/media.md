# Media frames

Put every image and video in a frame that reserves its space, loads from a mosaic, and ships as AVIF.

Every image and video in Mira lives in a frame, and the frame is the only decoration. Each part of it does a job:

- **Reserved space.** The compiler writes the exact width and height, so the page never jumps.
- **Pixel mosaic.** An 8×8 mosaic of the image's real colors paints first, then resolves into the photo in steps. It is the loading state, built from the logo's pixel module.
- **Corner handles.** They appear only when the image opens full size, so they tell the reader it is clickable.
- **Hairline border.** It is the only line, and it doubles as the focus ring.
- **Caption line.** Mono text that holds the real caption and credit.

## Images

```html
<mira-frame src="./pipeline.png" alt="Build pipeline" caption="Cold build, 1,000 pages" credit="Mira" zoom></mira-frame>
```

In Markdown, keep the whole `<mira-frame>` tag on one line. Markdown only treats a line as HTML when the opening tag is complete on that line, so a tag split across lines renders as text. In `.mira` templates, any layout works.

| Attribute | Meaning |
| --- | --- |
| `src` | The file. `./x.png` is relative to the file that contains the frame, `/x.png` is under `public/`, and `@/x.png` is under the project root |
| `alt` | Required. Describe the image, or use `alt=""` when it is decoration |
| `caption` | Optional caption shown under the frame |
| `credit` | Optional credit, shown after the caption |
| `zoom` | The frame links to the original at full size and shows corner handles |

Frames take PNG, JPEG, GIF, WebP, AVIF, and SVG. A missing `alt` fails the build with the file and a hint.

## Markdown images

Images in Markdown become frames automatically. The title becomes the caption:

```markdown
![Build pipeline](./pipeline.png "Cold build, 1,000 pages")
```

An image alone in its paragraph replaces the paragraph, since a figure cannot sit inside one.

## Video

```html
<mira-frame src="./demo.mp4" alt="The editor saving a page" width="1920" height="1080" poster="./demo.png" caption="Live reload"></mira-frame>
```

Video frames use the same parts. The poster image's mosaic is the loading state, a rounded pill plays and pauses, a thin progress line runs along the bottom edge, and the corner handles turn ember while the video plays. Mira cannot read a video's size, so `width` and `height` are required. The controls are a small script that loads only on pages with a video frame.

## Output

Each file is written once, however many pages use it, under a readable name with a short content hash:

```text
/media/pipeline-7kq2.avif
/media/pipeline-7kq2.png
```

Raster images are encoded to AVIF beside the original, and pages serve the AVIF with the original as a fallback. Encoding is pure Rust and cached by content hash in `.mira/cache/media`, so a second build performs no encodes. If the AVIF would be larger than the original, Mira ships the original alone.

## media.json

Every build with frames writes `/media.json`, a manifest of every file:

```json
{
  "id": "pipeline-7kq2",
  "kind": "image",
  "width": 1200,
  "height": 630,
  "formats": [
    { "src": "/media/pipeline-7kq2.avif", "format": "avif", "bytes": 27272 },
    { "src": "/media/pipeline-7kq2.png", "format": "png", "bytes": 336236 }
  ],
  "alt": "Build pipeline",
  "caption": "Cold build, 1,000 pages",
  "pages": ["/", "/posts/ship-less/"]
}
```

Hosts and agents read it, and `llms.txt` links it.

## The rule for decoration

Delete it. If something breaks (layout shift, state, clarity, or a click target), it stays. Otherwise it goes. One hue per frame, hairlines only, and no shadows or gradients on media.
