var v = from x in y
        let z = () =>
        {
            // Inside this query 'from' is a keyword, and is not a legal type
            // parameter name.
            void X<[] from>() { }
        }
        select x;