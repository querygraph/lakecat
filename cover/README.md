# LakeCat Cover

The final cover is `lakecat-cover.png`. It uses the published LakeCat
announcement headboard as its visual source and the reusable First Pair Press
publisher mask from `~/src/firstpair/logo/firstpair-publisher-mask.png`.

- Source headboard: `lakecat-blog-headboard.jpg`
- Source URL: <https://digitalpress.fra1.cdn.digitaloceanspaces.com/cz6pt2z/2026/07/LakeCat-on-Como.jpg>
- Generated portrait art: `lakecat-cover-art.png`
- Final composed cover: `lakecat-cover.png`

The portrait-art prompt was:

> Recompose the landscape LakeCat headboard as 2:3 portrait full-bleed cover
> art. Preserve the Lake Como golden-hour setting, lake steamer, cat captain at
> the wheel, mountain town, reflections, and whimsical big-cat language.
> Remove every word, logo, ship name, and sign. Simplify the animal crowd and
> leave calmer upper and lower regions for exact typography and a publisher
> mark. Add no unrelated subjects.

The generated art intentionally contains no lettering. Exact title, subtitle,
author, and publisher-seal placement are reproducible with:

```sh
uv run --no-project --with pillow python cover/make-cover.py
```
