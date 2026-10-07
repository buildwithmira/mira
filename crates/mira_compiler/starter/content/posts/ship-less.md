---
title: Ship less
description: Every byte sent to a browser must justify itself. Here is what this page actually ships.
date: 2026-09-21
---

This page ships no application JavaScript. The runtime that coordinates
transitions and prefetching is smaller than most favicons.

- Pages are rendered to HTML at build time.
- Critical CSS is inlined, so there is no render blocking request.
- Links are prefetched on hover, so the next page is usually ready before
  the click lands.
