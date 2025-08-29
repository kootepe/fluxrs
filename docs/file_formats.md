---
title: File formats
---
# File formats
## LI-7810
- Works with the files directly downloaded from the analyzer.
- Columns positions are read from the header so old formats should also work.
- Only SECONDS, NANOSECONDS, DIAG, CH4, CO2 and H2O columns are used.

## LI-7820
- Works with the files directly downloaded from the analyzer.
- Columns positions are read from the header so old formats should also work.
- Only SECONDS, NANOSECONDS, DIAG, N2O and H2O columns are used.

# Cycle / time data
2 file formats are supported.
### The "default" format

```
plot_id,start_time,close_offset,open_offset,end_offset
23,2024-03-22 14:00:00,60,120,420,480
37,2024-03-22 14:00:00,60,120,420,480
```

- start_time
  - if you have only recorded the time of the chamber close, set to chamber
  close - 60 seconds
- close_offset
  - Offset of chamber close from the start_time in seconds
- open_offset
  - Offset of chamber open from the start_time in seconds
- end_offset
  - Offset of cycle end from the start_time in seconds
  - Set to open_offset + 60 seconds if not set

### The manual measurement format
This is the format we use for collecting measurements manually in the field.

```
date,YYMMDD
measurement_time_in_seconds,120
instrument_model,LI-7810
instrument_serial,TG10-01420
plot_id,start_time,snow_depth
12,1341,6
13,1344
14,1347,6
15,1350
```

- plot_id
  - Arbitrary id of the measurement plot
  - plot_id needs to be possible to tie in with chamber height data
- start_time
  - Time when chamber was placed down
- snow_depth
  - Height of snow inside the chamber in centimeters (optional)
  - Snow depth will be set to 0 when if this column can't be parsed.

# Chamber height data
# Meteo data
- Simple format for air temperature and pressure data
- Air temperature in C°
- Air pressure in hPa


```
datetime,air_temperature,air_pressure
YYYY-MM-DD HH:MM:SS,10,994
```


# Chamber metadata
Some info about your plots / chambers.
For Box type chamber, the value isn't used, so you can either use a placeholder
value or leave it empty
For Cylinder type chamber, width and length aren't used so you can again either
use a placeholder value or leave them empty.


```
plot_id,shape,diameter,height,width,length
12,cylinder,24,12,
13,box,,1,1,1
```
