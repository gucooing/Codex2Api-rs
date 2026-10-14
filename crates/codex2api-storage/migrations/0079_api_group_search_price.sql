ALTER TABLE api_groups ADD COLUMN search_price_nano_usd INTEGER
    CHECK(search_price_nano_usd IS NULL OR
        (provider_id='chatgpt' AND typeof(search_price_nano_usd)='integer' AND search_price_nano_usd>=0));
