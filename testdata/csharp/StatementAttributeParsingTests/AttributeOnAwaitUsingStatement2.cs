
class C
{
    async void Goo()
    {
        [A]await using (null) { }
    }
}