
partial class PartialPartial
{
    int i = 1;
    partial partial void PM();
    partial partial void PM()
    {
        i = 0;
    }
    static int Main()
    {
        PartialPartial t = new PartialPartial();
        t.PM();
        return t.i;
    }
}
