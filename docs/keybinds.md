---
title: Keybindings

---


## Keyboard Shortcuts

The chamber flux validator comes with a set of keyboard shortcuts to speed up navigation and QC work. These only work in the `Validate measurements` panel

---

## Navigation between cycles

| Key               | Action             | Description                                     |
| ----------------- | ------------------ | ----------------------------------------------- |
| `→` (Right Arrow) | **Next Cycle**     | Move to the next chamber measurement cycle.     |
| `←` (Left Arrow)  | **Previous Cycle** | Move to the previous chamber measurement cycle. |

---

## Zoom / lag adjustment tools

These shortcuts control how the lag increment/decrement behaves and help you tune lags for the current cycle.

| Key          | Action                       | Description                                                                                                                                                                   |
| ------------ | ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Z`          | **Zoom To Measurement**      | Cycle the *zoom / lag focus mode* between different parts of the measurement (e.g. full cycle vs. opening/closing segment). This changes how `↑` / `↓` affect open/close lag. |
| `↑`          | **Increment Lag**            | Shift the relevant lag *forward* by 1 second, depending on the current zoom mode. Also recenters calculation ranges and updates plots.                                        |
| `↓`          | **Decrement Lag**            | Shift the relevant lag *backward* by 1 second, depending on the current zoom mode. Also recenters calculation ranges and updates plots.                                       |
| `Ctrl` + `L` | **Search Lag**               | Automatically search for a new open lag for the main gas in the current cycle and update plots.*                                                                              |
| `S`          | **Search Lag From Previous** | Use the previous **valid** cycle with the same chamber ID to infer a suitable open lag for the current cycle (based on the previous cycle’s open lag and current peak).**     |
| `R`          | **Reset Cycle**              | Reset processing parameters for the current cycle to their default state (lags, etc.).                                                                                        |

\* Looks for the maximum gas value in the last 1/4th of the currently visible
data.

\*\* Looks for the maximum gas value +-5 from the index of the previous valid measurement

---

## Validity & QC flags

| Key | Action                     | Description                                                                                         |
| --- | -------------------------- | --------------------------------------------------------------------------------------------------- |
| `I` | **Toggle validity**        | Toggle overall validity of the current cycle (valid ↔ invalid).                                     |
| `B` | **Toggle bad measurement** | Mark or unmark the current cycle as “bad” (manual QC flag). Bad measurements are hidden by default. |

---

## Filtering what is visible

These keys control which cycles are shown based on their QC flags.

| Key | Action                   | Description                             |
| --- | ------------------------ | --------------------------------------- |
| `Q` | **Toggle Hide Valids**   | Show/hide cycles marked as **valid**.   |
| `W` | **Toggle Hide Invalids** | Show/hide cycles marked as **invalid**. |
| `E` | **Toggle Show Bad**      | Show/hide cycles flagged as **bad**.    |

---

## Windows & panels

| Key  | Action                                 | Description                                                    |
| ---- | -------------------------------------- | -------------------------------------------------------------- |
| `F1` | **Toggle settings panel**              | Show/hide the settings panel.                                  |
| `F2` | **Toggle legend window**               | Show/hide the plot legend window.                              |
| `F3` | **Toggle cycle details window**        | Show/hide a detailed information window for the current cycle. |
| `F4` | **Toggle plot size adjustment window** | Show/hide the plot width/size adjustment window.               |
| `F5` | **Toggle lag plot**                    | Show/hide the lag plot.                                        |

---

## Advanced / configurable actions (no default keybinding)

The following actions exist in the application but **do not** have a default keybinding. They can be bound via the key bindings configuration file if you need them on the keyboard:

* **Debug / view modes**

  * `ToggleShowLinear` – Toggle linear model plots.
  * `ToggleShowRobLinear` – Toggle robust linear model plots.
  * `ToggleShowPoly` – Toggle polynomial model plots.
  * `ToggleShowResiduals` – Toggle residual bar plots.
  * `ToggleShowStandResiduals` – Toggle standardized residual plots.

* **Per-gas validity**

  * `ToggleCH4Validity` – Toggle validity for CH₄ fluxes only.
  * `ToggleCO2Validity` – Toggle validity for CO₂ fluxes only.
  * `ToggleH2OValidity` – Toggle validity for H₂O fluxes only.
  * `ToggleN2OValidity` – Toggle validity for N₂O fluxes only.

* **Deadband controls (global & per gas)**

  * `IncrementDeadband` / `DecrementDeadband`
  * `IncrementCH4Deadband` / `DecrementCH4Deadband`
  * `IncrementCO2Deadband` / `DecrementCO2Deadband`
  * `IncrementH2ODeadband` / `DecrementH2ODeadband`
  * `IncrementN2ODeadband` / `DecrementN2ODeadband`

You can customize bindings so that no two actions share the same key+modifier combination; the code automatically removes previous assignments that clash with a new one.

