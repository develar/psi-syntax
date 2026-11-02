
                var r = from s in a
                        where s is X<T> // should disambiguate as a type here
                        where M(s)
                        select s as X<T>;