class C
{
    void extension() { extension(); }
    void M()
    {
        extension extension = null;
    }
}