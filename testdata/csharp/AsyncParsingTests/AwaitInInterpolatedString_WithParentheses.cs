class C
{
    async void M()
    {
        var v = $"{await (M())}";
    }
}