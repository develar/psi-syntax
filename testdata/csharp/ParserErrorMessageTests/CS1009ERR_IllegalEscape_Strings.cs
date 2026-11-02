
class Program
{
    static void Main()
    {
        string s;
        s = "\u";
        s = "\u0";
        s = "\u00";
        s = "\u000";
        
        s = "a\uz";
        s = "a\u0z";
        s = "a\u00z";
        s = "a\u000z";
    }
}
