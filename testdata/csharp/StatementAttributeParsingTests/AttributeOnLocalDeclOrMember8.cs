
class C
{
    void Goo(System.IAsyncDisposable d)
    {
        [A]await using var i = d;
    }
}