(int arg,
                            Func<int, Color> colorFunc = (Color c = Color.Red) => c switch 
                                                                           { 1 => Color.Green, 2 => Color.Red, 3 => Color.Blue }) =>
            { return colorFunc(arg); }