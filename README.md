# Foliot
A simple time tracking tool to keep track of your working or studying time.

You can create different namespaces for different purposes.
The data as human readable and editable [YAML](https://yaml.org/) (run `foliot path` to get the path).

## Examples:

### Using the Timer
Use the `clockin` subcommand to start the timer.
You can specify a namespace, e.g. `work` or skip the argument ad use the `default` namespace:
```sh
foliot --namespace work clockin
```

Now after doing some work you can end the session and add a comment:
```sh
foliot -n work clockout "Procrastinating on reddit"
```

You can also add minutes to the clock afterwards.
If you don't specify a starting time it will be calculated from the current time:
```sh
foliot clock 2.5 --starting 15:30
```

### Getting the Data
To list all entries for a namespace use `show`:
```sh
foliot -n work show
```

The `summarize` subcommand provides an overview over the past months
```sh
foliot summarize
```


There are many more features, like editing and git support.
Run `foliot --help` to see them.
