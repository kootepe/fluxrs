---
title: Command line mode
---

---

# **Fluxrs – Command line mode manual**

The commandline mode is now provided in a separate binary called `fluxrs_cli`
due to windows being janky with having a GUI and commandline mode in the same
binary.

`fluxrs` has a command-line mode for:

* Creating projects
* Uploading raw data (cycles, gas, height, meteo, chamber metadata)
* Running the processing pipeline

---

## **Table of Contents**

1. [Commands Overview](#commands-overview)
2. [Create a new project](#create-a-new-project)
3. [Uploading Data](#uploading-data)

   * [Supported Data Types](#supported-data-types)
   * [Upload Command](#upload-command)
4. [Running the Processor](#running-the-processor)
5. [Datetime Formats](#datetime-formats)
6. [Incremental File Uploading](#incremental-file-uploading)
7. [Examples](#examples)

---

# **Commands Overview**

### Top-level structure

```
fluxrs_cli
  project  <subcommands>
  upload   <dataset>
  run      <options>
```

---


# **Create a New Project**

```bash
fluxrs_cli project create --name NAME \
                      --instrument TYPE \
                      --serial SERIAL \
                      --main-gas GAS \
                      --deadband SECS \
                      --min-calc-len SECS \
                      --mode MODE \
                      --tz TIMEZONE \
```

### **Arguments**

| Flag             | Description                                    |
| ---------------- | ---------------------------------------------- |
| `--name`         | Project name (unique in the DB)                |
| `--instrument`   | Instrument type (enum)                         |
| `--serial`       | Instrument serial number                       |
| `--main-gas`     | `CO2`, `CH4`, etc.                             |
| `--deadband`     | Deadband threshold (seconds)                   |
| `--min-calc-len` | Minimum calculation duration (seconds)         |
| `--mode`         | Processing mode (`bestr`, `deadband`, etc.)    |
| `--tz`           | Timezone (IANA format, e.g. `Europe/Helsinki`) |

### **Example**

```bash
fluxrs_cli project create \
    --name ForestPlotA \
    --instrument LI7810 \
    --serial 12345 \
    --main-gas CO2 \
    --deadband 60 \
    --min-calc-len 300 \
    --mode deadband \
    --tz Europe/Helsinki
```

---

# **Uploading Data**

Uploads do **not** perform processing — only ingestion.

```
fluxrs_cli upload <dataset> [OPTIONS]
```

## **Supported Data Types**

| Dataset | Command                | Table    |
| ------- | ---------------------- | -------- |
| Cycles  | `fluxrs_cli upload cycle`  | `cycles` |
| Gas     | `fluxrs_cli upload gas`    | `gas`    |
| Height  | `fluxrs_cli upload height` | `height` |
| Meteo   | `fluxrs_cli upload meteo`  | `meteo`  |

## **Upload Command Syntax**

```bash
fluxrs_cli upload <cycle|gas|height|meteo> \
    --project NAME \
    --inputs <FILES...> \
    [--newest] \
    [--tz TZ] \
    [--db DB_PATH]
```

### **Options**

| Flag            | Description                                              |
| --------------- | -------------------------------------------------------- |
| `--project, -p` | Project to upload into                                   |
| `--inputs, -i`  | One or more file paths or glob patterns (`"data/*.csv"`) |
| `--newest, -n`  | Upload only files newer than last DB timestamp           |
| `--tz, -z`      | Override timezone for timestamps (project timezone used by default)                         |

### **Example**

Upload all gas files:

```bash
fluxrs_cli upload gas -p ForestPlotA -i "gas/*.txt"
```

Upload only new meteo files:

```bash
fluxrs_cli upload meteo -p ForestPlotA -i "meteo/*.csv" -n
```

Upload cycle data with timezone:

```bash
fluxrs_cli upload cycle -p ForestPlotA -i "cycles/*.csv" -z Europe/Helsinki
```

---

# **Running the Processor**

The processor reads already-uploaded data, runs queries, processes fluxes, and writes results.

```bash
fluxrs_cli run \
    --project NAME \
    [--instrument TYPE] \
    [--start DATETIME] \
    [--end DATETIME] \
    [--newest] \
    [--tz TZ] \
    [--init] \
```

### **Options**

| Flag               | Description                                  |
| ------------------ | -------------------------------------------- |
| `--project, -p`    | Project to process                           |
| `--instrument, -i` | Override instrument type                     |
| `--start, -s`      | Start datetime range                         |
| `--end, -e`        | End datetime range                           |
| `--newest, -n`     | Use newest measurement day as start          |
| `--tz, -z`         | Force timezone for interpretation            |
| `--init`           | Start processing immediately (legacy option) |

### **Examples**

Process entire data range:

```bash
fluxrs_cli run -p ForestPlotA
```

Process last day only:

```bash
fluxrs_cli run -p ForestPlotA -n
```

Process a specific time window:

```bash
fluxrs_cli run -p ForestPlotA \
    -s "2024-06-10T00:00:00Z" \
    -e "2024-06-12T23:59:59Z"
```

---

# **Datetime Formats**

`fluxrs_cli` supports many formats:

### ISO 8601

```
2024-06-12T14:20:00Z
2024-06-12T14:20:00+02:00
```

### RFC 2822

```
Wed, 12 Jun 2024 14:20:00 +0000
```

### Common local formats

```
2024-06-12 14:20:00
2024/06/12 14:20:00
2024-06-12
2024-06
12-06-2024 14:20:00
06/12/2024 14:20:00
```

If no timezone is given, it assumes **local timezone**, then converts to UTC.

---

# **Incremental File Uploading**

If `--newest` is used:

1. The program looks in the DB for the latest timestamp in the relevant table (`gas`, `cycles`, etc).
2. The upload only includes files whose filesystem modification time (mtime) is **>=** that timestamp.
3. If no previous data exist, all files are uploaded.

### Example

```bash
fluxrs_cli upload gas -p ForestPlotA -i "gas/*.txt" --newest
```

---

# **Full Usage Examples**

## **1. Create a project**

```bash
fluxrs_cli project create \
    --name BogExperiment \
    --instrument LI7810 \
    --serial L1234 \
    --main-gas CH4 \
    --deadband 45 \
    --min-calc-len 240 \
    --mode bestr \
    --tz Europe/Helsinki
```

## **2. Upload cycle and gas files**

```bash
fluxrs_cli upload cycle -p BogExperiment -i "cycles/*.csv"
fluxrs_cli upload gas   -p BogExperiment -i "gas/*.txt"
```

## **3. Upload only new files next time**

```bash
fluxrs_cli upload gas -p BogExperiment -i "gas/*.txt" -n
```

## **4. Run processing for entire project**

```bash
fluxrs_cli run -p BogExperiment
```

## **5. Run for a specific date**

```bash
fluxrs_cli run -p BogExperiment -s "2024-06-12" -e "2024-06-13"
```

---

# **Support**

If you need additional documentation or automation help (e.g., systemd service, Dockerfile, GUI wrapper), feel free to ask!

---
