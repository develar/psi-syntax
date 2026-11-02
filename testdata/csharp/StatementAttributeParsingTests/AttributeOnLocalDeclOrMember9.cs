
class C
{
    async void Goo(System.IAsyncDisposable d)
    {
        [A]await using var i = d;
    }
}