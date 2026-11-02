
class C
{
    async void f()
    {
        int c = await g() || await g();
    }
}
