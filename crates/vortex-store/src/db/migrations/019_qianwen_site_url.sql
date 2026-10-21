-- 千问个人版已迁至 qianwen.com（原 tongyi.aliyun.com，品牌由「通义」
-- 更名「千问」）。修正 003 种子的名称与地址；WHERE 守卫避免覆盖用户改动。
UPDATE free_token_sites
SET name = '千问（网页版）',
    home_url = 'https://www.qianwen.com'
WHERE id = 'cn-web-tongyi'
  AND home_url = 'https://tongyi.aliyun.com';
