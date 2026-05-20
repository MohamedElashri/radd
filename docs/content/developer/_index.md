+++
title = "Developer"
description = "Architecture and contribution notes for people changing radd."
weight = 30
sort_by = "weight"
+++

This section explains how `radd` is shaped internally and how to make changes without breaking its safety model.

The codebase is intentionally small: planning is deterministic, subprocess execution is isolated, and ROOT-specific behavior stays behind external command boundaries.
