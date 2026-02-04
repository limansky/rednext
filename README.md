rednext
=======

[![Build Status](https://github.com/limansky/rednext/actions/workflows/ci.yml/badge.svg)](https://github.com/limansky/rednext/actions/workflows/ci.yml)

Rednext is a simple random task list organizer.

Currently it supports only SQLite as a backend database, but more databases very possible will be added in the future.

Basic usage
-----------

To create a new task list database, run:

```bash
rednext new mytasks
```

You will be asked for the database structure. Any database have to have at least one field. You can also specify CSV file
to import tasks from. You can also import data later using `rednext items mytasks import <file>` command.


Now you can manage your items in the database. For example to add a new item to the task list, run:

```bash
rednext items mytasks add
```

To see all items in the database, and the completion progress, run:

```bash
rednext items mytasks list
```

To get a random item from the task list, run:

```bash
rednext items mytasks get-random
```

You can also mark items as done or undone:

```bash
rednext items mytasks get <item-id>
```

For more commands and options, run:

```bash
rednext --help
```
