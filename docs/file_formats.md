# Gas Analyzer data
## LI-7810
Only the file format that's directly downloaded from the analyzer is supported.
## LI-7820
Only the file format that's directly downloaded from the analyzer is supported.
# Cycle / time data
2 file formats are supported.
## The default format

```
start_time,close_offset,open_offset,end_offset
2024-03-22 14:00:00,60,120,420,480
2024-03-22 14:00:00,60,120,420,480
```

- start_time
  - if you have only recorded the time of the chamber close, set to chamber
  close - 60 seconds
- close_offset
  - Offset of chamber close from the start_time in secods
- open_offset
  - Offset of chamber open from the start_time in secods
- end_offset
  - Offset of cycle end from the start_time in secods
  - Set open_offset + 60 seconds if not set

## The manual measurement format

```
date,YYMMDD
measurement_time_in_secods,120
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
- start_time
  - Time when chamber was placed down
- snow_depth
  - Height of snow inside the chamber in centimeters (optional)

# Chamber height data
# Meteo data
# Chamber metadata
