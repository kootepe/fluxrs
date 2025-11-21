# Release Notes

## Next release
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
