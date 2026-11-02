
class C
{
    void Goo(System.IDisposable d)
    {
        [A]using var i = d;
    }
}