# Sayvane website

Static landing page: plain HTML, CSS and JS with no build step, so Vercel serves it as is.

## Deploy on Vercel

1. Import the repository in Vercel.
2. Set **Root Directory** to `site`, **Framework Preset** to `Other`, and leave the build command empty.
3. Deploy. `vercel.json` adds clean URLs (`/privacy`), cache and security headers, and the download redirects.

Or from this folder: `npx vercel --prod`.

## Before going live

- **Domain**: canonical URLs, `og:image`, `sitemap.xml`, `robots.txt` and `llms.txt` assume
  `https://sayvane.vercel.app`. Search and replace it if the project gets a different domain.
- **Downloads**: `/download/mac-arm`, `/download/mac-intel` and `/download/windows` redirect to
  GitHub Releases in `vercel.json`. Point them at the real installer URLs once releases exist.
- After deploying, submit `/sitemap.xml` in Google Search Console and Bing Webmaster Tools,
  and test the page with the Rich Results Test (FAQ and SoftwareApplication markup).

## Updating screenshots

The images in `assets/shots/` are real captures of the app UI running in mock mode
(`pnpm dev:mock`, then open `/?page=home|history|settings`). Re-capture at 2x and convert to
WebP at 1600px wide. `assets/og.png` is the 1200x630 social preview.

## Notes

- Manrope is self-hosted (`assets/fonts/`, SIL Open Font License), so the site makes no
  third-party requests.
- Respects `prefers-reduced-motion`: the demo shows a still frame and the tagline renders fully lit.
