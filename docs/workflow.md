---
title: Workflow
---
This page will explain a basic workflow with the app from starting a project
to starting processing.
# Initiate project tab
## Creating a new project
- __Project name__:
  - Give the project a name
- __Select instrument__:
  - Select the model of the instrument you are using
- __Instrument serial__:
  - Give the serial of your main instrument
- __Select gas__:
  - Give the main gas used for evaluating fluxes
  - eg. LI7810 measures CH4 and CO2, and if the CH4 measurement is typically
  more linear, it can be used to evaluate the CO2 measurement
- __Minimum calculation data length in seconds__:
  - When flux finding mode is Best Pearson's R, the shortest possible length of
    time where flux will be measured will be set to this value.
  - When flux finding mode is After deadband, flux will be calculated from this
    length of time after deadband.
- __Deadband in seconds__:
  - How many seconds to skip after the beginning of each measurement
- __Select flux finding mode__:
  - Best Pearson's R
    - Calculates flux where the measurement is most linear.
    - If your minimum calculation data length is very short (< 1 minute) and your
    measurements are long (> 10 minutes), this will cause data initiation to take quite
    long as it checks every measurement second by second.

# Upload files to db tab
Here you can upload different data files into the DB. Only files that are
__REQUIRED__ are the instrument data files and cycle file(s). Default values
will be used for chamber dimensions, air pressure and air temperature.

__Do not upload height files for manual measurements.__

Read more about file formats [here](file_formats).

# Initiate measurements tab

This will automatically calculate all of your fluxes.

Pick a start date and an end date for the range which you want to calculate
fluxes from and hit __Initiate measurements__.

If you selected Best pearson's R in as the flux finding mode in the project
initiation and you have a lot of long measurements with a short minimum
calculation data length, this can take quite long as the program goes through
each measurement second by second.

You will get some progress messages and maybe errors in the Log Messages just
under all of the buttons.

You might experience crashing if you try to load a lot of (several months of
24/7 measurements) measurements at once. For manual measurements you can process
all of your measurements at once.

## Recalculate
Hitting recalculate will recalculate all of your fluxes that have new height/volume data. Meaning that you can process all of your fluxes without having any height/volume data, and then add that data later.
# Load measurements
This just loads measurements into memory for processing. My PC can handle 5000~
15 minute measurements pretty well. If the program feels laggy, load less data
from here.


# Validate measurements
Here you can finally start processing your measurements.
