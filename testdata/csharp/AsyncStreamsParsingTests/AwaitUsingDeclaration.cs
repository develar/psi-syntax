
class C
{
    async void M()
    {
        await using (var x = this)
        {
        }
    }
}
