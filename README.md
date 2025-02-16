App for calculating static chamber fluxes.

Usage:

```
fluxrs "<gas path glob>" "<time path glob>"
eg.
fluxrs "data/24*.DAT" "time_data/24*",
```

Roadmap, mostly just for myself so I won't sidetrack too much.
- 0.1.0
  - Calculate fluxes from cycles and gas measurements
  - Display simple plots
- 0.2.0
  - Calculate fluxes with temperatures and pressures
