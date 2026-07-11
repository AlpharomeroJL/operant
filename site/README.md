# Operant Documentation Site

This directory contains the plain static HTML documentation site for Operant, deployed to GitHub Pages.

## Structure

- `index.html` - Landing page with links to Guides, Cookbook, Benchmarks, and Architecture
- `style.css` - Shared stylesheet with light and dark mode support
- `guides/` - Placeholder guide pages (install, first-workflow, scheduling, privacy)

## Building and Deploying

The site requires zero dependencies and zero build steps. All files are plain HTML and CSS.

To view locally, open `index.html` in your browser.

The site is automatically deployed to GitHub Pages on push to main via `.github/workflows/pages.yml`. The workflow:
1. Uploads the `site/` directory as a GitHub Pages artifact
2. Deploys it to the GitHub Pages environment

## Design

The site is designed to work in both light and dark modes, respecting the user's system preference via `prefers-color-scheme`. All links use relative paths and work correctly whether served from the docs root or a subdirectory.

## Contributing

Guide content placeholders in `guides/` will be filled by D1B. Each guide is a complete HTML stub with navigation and styling already in place.
