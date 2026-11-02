using System;
public class Test
{
    public void M()
    {
               var v = int.MaxValue; // error
        bool b = false;
        string s = "true";
        Boolean.TryParse(s, out b); // ok
        bool.TryParse(s, out b);  // error
    }
}