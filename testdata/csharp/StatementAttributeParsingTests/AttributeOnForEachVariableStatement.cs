
class C
{
    void Goo((int, string)[] vals)
    {
        [A]foreach (var (i, s) in vals) { }
    }
}