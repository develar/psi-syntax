class C
{
    void f()
    {
        async delegate () {
            await 1;
            delegate () {
                int await;
            };
        };
    }
}