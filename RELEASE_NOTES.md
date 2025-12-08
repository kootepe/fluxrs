# Release Notes

## v0.4.9
- Added some db indexes for faster queries
  - with migrations
- Remove lags from fluxes table as they are linked from another table
  - Slight performance increase + smaller db size
  - with migrations
- File reading no longer frees the UI too early
- Added sanity checks to expflux
- Searching lag from previous cycle now actually uses the previous cycles lag
- Added custom errors for flux calculations
- Sanity checks for instrument serials
- Added ability to adjust plot point size in settings
- A lot of internal refactoring, giving various parts of the GUI clearer
responsibilities

## v0.4.8
- All data formats now allow inserting the same file multiple times, but only
rows that are new will be inserted.
- fluxrs/fluxrs no longer launches fluxrs_cli. If you want to use the CLI
functionality, you need to launch fluxrs_cli directly.

## v0.4.7
- Split commandline into a separate crate and binary
- Fluxrs is now packaged with gui (fluxrs) and cli (fluxrs_cli) binary
  - Having cli and gui in the same binary doesn't work on windows without
  annoying jank, this is the fix
- Running fluxrs (the gui binary) with arguments now runs the fluxrs_cli binary
  which provides the cli functionality


## v0.4.6
- Fix close lag adjusting
  - previously close lag wouldnt move if open lag was at the bound
- Lag plot is now toggleable via settings or keybind
- Chamber now knows if its a default value or actual data
- Moved chamber dimensions and meteodata to be visible by default in cycle
details
- Meteodata distance from target now has a sign, eg. can be negative or positive
  instead of just positive
  - Shown in cycle details
- Moved db logic to module

## v0.4.5
- Temperature and pressure now have a "distance from target"
  - eg. they report how far from the target timestamp they were
  - can be used for validation
  - for now still enforces that temperature and pressure must be within
  30minutes of the target

## v0.4.4
- Temperature and pressure are now looked for independently
  - before, they had share the same timestamp
  - now looks for nearest non-nan value within 30minutes

## v0.4.3
- Cleaning up the validation app struct
  - move plot toggling to separate struct
  - move model fit toggling to separate struct
- Lag times can't be adjusted beyond the plot bounds anymore
- Fixed parsing of null values in meteodata
  - Meteodata now knows if it's a default value
    - Added pressure_source and temperature_source columns
      - default means it's a default value by the program
      - raw means it's a data value from db
    - Added pressure and temperature to cycle details
      - highlighted as orange when it's a default value


## v0.4.2
- Fix bad SQL in project loading
  - instrument was being linked via the wrong id
- Minor optimization from remove unnecessary db queries
- Add cycle start timestamp to display window

## v0.4.1
- Added sanity checks in linear regression and OLS for robust linear
- Better error handling for cycle processing

## v0.4.0
- Performance improvements by removing clones from structs are now copy
- Add chamber metadata pushing to cmd mode
- Fixed a bug in the linear model, y deviation was being calculated by using
x_avg
  - There since 0.2.0
- Added ExponentialFlux and ExpReg for exponential flux calculation
- Further optimizing by removing doubled function runs in validation
- Remove model specific calc ranges from db
  - Calc range is the same for each model, it can only wary per gas
- Residual plots were missing proper y axis titles
- All data is also now processed as unix time and displayed in the selected
timezone
- Add chamber volume and area, and concentration at t0 to details window

## v0.3.1
**BREAKING CHANGES**
Reworked the db schema quite a bit to allow easier deletion of data.

Hopefully the last time i'll break semver.

- Polynomial flux now initiates slope at t0
  - used to be in the middle of the measurement causing linear nad polynomial
  flux to be essentially the same
- Added links between tables
  - Allows for smaller db size and easier deletion of data
- Split the app into two separate crates between the GUI and calculation logic
  - In theory allows for others to also use it more easily
    - Unfortunately almost everything in core is still public so it's not that
    possible yet
- Added better logic for recalculation of fluxes
- Added UI for a deleting projects
- Added UI for deleting files and their data
  - some these trigger flux recalculation
- Added CV to model attributes



## v0.2.0
Various usability features, nonexhaustive list:

- Added various units for fluxes
- Better data downloading
- calculation area now moves more logically with adjuted lags
- enforce UTF8 for input files
- better log messages
  - still needs improving
- app now remembers last selected date range
- db viewer now only shows data for current project
- added timezone support
  - all files ask for timezone when uploading
  - project timezone used for displaying
  - all data still stored internally as UTC

## v0.1.0
First pre-alpha release to agonize the people....
Expect bugs and usability issues :-)

Need to set up automated releases at some point.
