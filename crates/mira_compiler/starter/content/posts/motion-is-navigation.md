---
title: Motion is navigation
description: Why page transitions belong to routing, and how shared elements show readers where they went.
date: 2026-10-06
---

A transition answers a question the user did not have to ask: *where did I
just go?* When a list item grows into the page it opens, the relationship
between the two pages is obvious.

## Shared elements

Give an element the same `mira-morph` name on two pages and Mira compiles it to
a `view-transition-name`. The browser does the rest, with no JavaScript
animation library.

## Respecting the reader

Readers who ask for reduced motion get a short crossfade instead. Keyboard
focus lands on the new page's heading, so nobody is left behind.
