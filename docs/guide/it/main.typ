// SPDX-License-Identifier: GPL-3.0-or-later
//
// Orchestra user guide — Italian (GUIDA.pdf). Mirrors docs/guide/en/main.typ file by file and
// heading by heading; `cargo xtask check` verifies the two trees have the same structure.

#import "/docs/guide/template.typ": guide

#show: guide.with(
  title: "Orchestra",
  subtitle: "Guida per l'utente",
  edition: "Edizione di anteprima · Fase 0 · generata da cargo xtask guides",
  lang: "it",
)

#include "chapters/01-introduction.typ"
