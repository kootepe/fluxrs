App for calculating static chamber fluxes.

copy repo and run ```cargo build --release``` to build.

Initiate new project on ```Select and initiate project ``` tab.
Give arbitrary project name. Select instrument from the dropdown. Give serial of
the instrument used. Select main gas, used for lagtime calculation.

```Upload files to db``` tab to upload files. Currently just upload Gas files
(gas analyzer outputs) and cycle files.

Cycle file format:
```
chamber_id,start_time,close_offset,open_offset,end_offset
1,2021-10-23 14:00:01,180,780,900
2,2021-10-23 14:15:01,181,780,901
```
Offsets are seconds added to the start_time.

```Initiate measurements``` tab to initiate measurements for manual validation.
Select start time and end time, all cycles that don't exist in db for current
project will be initiated.

```Load measurements``` tab to load measurements for manual validation.


Roadmap, mostly just for myself so I won't sidetrack too much.
- 0.1.0
  - simple linear model flux calculation
  - UI with egui and egui_plot
  - sqlite db
  - data initiation via commandline



