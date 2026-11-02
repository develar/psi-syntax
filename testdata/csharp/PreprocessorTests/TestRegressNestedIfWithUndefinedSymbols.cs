class A
    {
#if AAA
        static void M()
        {
            int x = 0;
#if BBB
            x =1;
#endif
            x =2;
        }
#endif
}